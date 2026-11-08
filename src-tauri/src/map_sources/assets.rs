use super::identity::map_label;
use crate::models::MapRepositoryConfig;
use crate::models::ReleaseAsset;
use crate::models::ReleaseInfo;
use crate::models::{AssetRule, AssetRuleType};

/// tag 模式下负责「从资产名里抽版本号」的那条规则。
///
/// 必须同时满足：是正则、能编译、至少有一个捕获组（优先 `(?P<version>…)`，
/// 否则取第 1 组）。锚定由调用方另外查 —— 这里只回答「能不能抽出版本」。
pub(super) fn version_rule(rules: &[AssetRule]) -> Option<&AssetRule> {
    rules.iter().find(|rule| {
        matches!(rule.rule_type, AssetRuleType::Regex)
            && regex::Regex::new(&rule.value)
                .map(|regex| regex.captures_len() > 1)
                .unwrap_or(false)
    })
}

/// 把一个 tag 的资产拆成「一个资产 = 一个版本」的 Release 列表。
///
/// ## 为什么是合成而不是新模型
///
/// 前端、安装流程（`stage_map_release`）、缓存字段、依赖快照全都按
/// `Vec<ReleaseInfo>` 走。合成一份形状相同的列表，等于让「一个 tag 装一张图的
/// 历史版本」在整条链路上零改动 —— 安装时按 `tag_name` 找到的那条 Release 里
/// 就只有一个资产，正是它自己。
///
/// ## 取哪个数字当版本
///
/// 从资产名里用作者写的正则抽（`version_rule`）。这比解析 tag 名可靠：
/// 一个 tag 里放的是多个文件，版本只在文件名里。抽出来的号同时充当
/// `tag_name` —— 前端拿它当「版本号」显示、当「回滚到这一版」的标识。
///
/// ## 作者排过序的表优先
///
/// `asset_versions` 非空时顺序与版本号都由它决定（末项最新，输出反转）：
/// 文件名里抽不出 `3.4.1-fix`，数字段比较也排不出「0.7 比 0.69 新」。表外
/// 资产不暴露；没有版本表时才按数字段降序。表里只存版本号，附件名由
/// `asset_rules` 的版本规则反转渲染出来（见 `asset_name_for_version`）。
pub(crate) fn synthesize_releases(
    map: &MapRepositoryConfig,
    tag: &str,
    releases: &[ReleaseInfo],
) -> Result<Vec<ReleaseInfo>, String> {
    let label = map_label(map);
    let Some(release) = releases
        .iter()
        .find(|release| release.tag_name.trim() == tag)
    else {
        return Err(crate::locale_tag::with_params(
            "mapSource.releaseTagNotFound",
            &[("map", &label), ("tag", tag)],
        ));
    };
    let Some(rule) = version_rule(&map.asset_rules) else {
        return Err(crate::locale_tag::with_params(
            "mapSource.assetRuleNeedsVersion",
            &[("map", &label)],
        ));
    };
    let regex = regex::Regex::new(&rule.value).map_err(|error| {
        log::error!("地图版本规则无法编译：地图={}，原因={}", label, error);
        crate::locale_tag::with_params("mapSource.assetRuleNeedsVersion", &[("map", &label)])
    })?;
    /*
     * 有版本表时，数组顺序与版本号均为权威；只暴露表内仍存在的资产。
     * 没有版本表时，兼容旧 Release，按显式 asset 规则扫描匹配附件。
     */
    let mut versions: Vec<(String, ReleaseAsset)> = Vec::new();
    for entry in map.asset_versions.iter().rev() {
        let version = entry.mapset_version.trim();
        if version.is_empty() {
            continue;
        }
        /* 表里没有附件名：按作者的 asset 规则把版本号渲染回文件名。规则反向渲染
         * 不出来时退一步，找版本段正好等于它的那个资产。 */
        let declared = asset_name_for_version(&map.asset_rules, version);
        let asset = declared
            .as_ref()
            .and_then(|name| release.assets.iter().find(|asset| &asset.name == name))
            .or_else(|| {
                release.assets.iter().find(|asset| {
                    matched_version(&label, &regex, asset).as_deref() == Some(version)
                })
            });
        let Some(asset) = asset else {
            log::debug!(
                "版本表里的版本在这一版里找不到对应资产，跳过：地图={label}，版本={version}"
            );
            continue;
        };
        push_version(&mut versions, version.to_string(), asset, &label)?;
    }
    let listed_count = versions.len();
    // 版本表存在时是唯一公开清单，表外附件不能自动暴露；只有旧格式没有版本表时，
    // 才扫描全部附件兼容历史地图。
    if map.asset_versions.is_empty() {
        for asset in &release.assets {
            let Some(version) = matched_version(&label, &regex, asset) else {
                continue;
            };
            push_version(&mut versions, version, asset, &label)?;
        }
    }
    if versions.is_empty() {
        return Err(crate::locale_tag::with_params(
            "mapSource.noAssetInTag",
            &[("map", &label), ("tag", tag)],
        ));
    }
    /* 作者声明的「这就是最新版」必须真的存在，否则界面会把一张不存在的版本
     * 标成最新，用户点安装却找不到资产。 */
    let forced = map
        .latest_mapset_version
        .as_deref()
        .map(str::trim)
        .filter(|version| !version.is_empty());
    if let Some(forced) = forced
        && !versions.iter().any(|(version, _)| version == forced)
    {
        return Err(crate::locale_tag::with_params(
            "mapSource.latestVersionMissing",
            &[("map", &label), ("version", forced)],
        ));
    }
    // 没有版本表时 listed_count 为 0，全部按数字段降序；有表时切片为空，
    // 保持作者给的顺序，不让旧标量把表尾挤下去。
    versions[listed_count..].sort_by(|(left, _), (right, _)| newest_first(left, right));
    if map.asset_versions.is_empty()
        && let Some(forced) = forced
        && let Some(index) = versions.iter().position(|(version, _)| version == forced)
    {
        // 置顶而不是重排：其余版本仍按数字顺序，作者只说「它是最新」。
        let entry = versions.remove(index);
        versions.insert(0, entry);
    }
    // tag 模式下这个字段是「最多暴露几个版本」，API 分页不会受此展示上限影响
    versions.truncate(map.max_release_count.clamp(1, 100) as usize);
    versions
        .into_iter()
        .map(|(version, asset)| {
            let mut assets = crate::archive::release_download::asset_set(&asset, &release.assets)?;
            // The selected primary stays first for tag-mode installation.
            assets.rotate_right(1);
            Ok(ReleaseInfo {
                // 多个版本共用一个真实 Release，ID 只能留空：留着会让它们撞键
                release_id: None,
                tag_name: version.clone(),
                version: Some(version.clone()),
                name: format!("v{version}"),
                // 版本详情里显示的是作者写给人看的说明，元数据块是给管理器的，
                // 不该跟着 body 一起流到界面上（它是 HTML 注释，但别指望渲染器）
                body: crate::map_note::strip_note_block(&release.body),
                published_at: release.published_at.clone(),
                // 渠道过滤已按作者声明跳过，这两个标记不能再参与判定
                prerelease: false,
                draft: false,
                assets,
            })
        })
        .collect()
}

/// 把版本号渲染回资产名：`^gaoda_v(?P<version>[0-9.]+)\.zip$` + `3.4.1-fix`
/// 得到 `gaoda_v3.4.1-fix.zip`。
///
/// 版本表里只存版本号（附件名由 asset 规则唯一决定），而规则本身是正则，
/// 这里做的是反过来渲染：取版本组前后的字面量、还原 `\.` 这类转义。规则里
/// 用到其它正则构造（字符类、分组、分支、量词）时不猜，返回 None —— 猜错会
/// 指向一个不存在的文件，宁可由调用方退回按版本段比对附件名。
fn asset_name_for_version(rules: &[AssetRule], version: &str) -> Option<String> {
    rules.iter().find_map(|rule| {
        let body = rule.value.trim().strip_prefix('^')?.strip_suffix('$')?;
        let start = body.find("(?P<version>")?;
        let rest = &body[start + "(?P<version>".len()..];
        // 版本组里可能套着非捕获组 `(?:...)`，逐层配平括号找它的结尾
        let mut depth = 1usize;
        let mut end = None;
        for (index, character) in rest.char_indices() {
            match character {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(index);
                        break;
                    }
                }
                _ => {}
            }
        }
        let prefix = literal_from_pattern(&body[..start])?;
        let suffix = literal_from_pattern(&rest[end? + 1..])?;
        Some(format!("{prefix}{version}{suffix}"))
    })
}

/// 把正则里的字面量段还原成原文；碰到真正的正则构造就放弃。
fn literal_from_pattern(pattern: &str) -> Option<String> {
    let mut out = String::with_capacity(pattern.len());
    let mut characters = pattern.chars();
    while let Some(character) = characters.next() {
        match character {
            '\\' => out.push(characters.next()?),
            '[' | '(' | ')' | '|' | '+' | '*' | '?' | '{' | '}' | '^' | '$' => return None,
            other => out.push(other),
        }
    }
    Some(out)
}

/// 按作者的版本规则从资产名里抽版本号；整串匹配才算命中。
///
/// `asset_matches` 用的是 `is_match`（子串命中即真），不锚定就会出现「同一个
/// 文件同时匹配出两个版本号」。解析阶段已经要求规则写成 `^…$`，这里再判一次
/// 是因为配置里的旧条目可能没经过那道校验。
fn matched_version(label: &str, regex: &regex::Regex, asset: &ReleaseAsset) -> Option<String> {
    let captures = regex.captures(&asset.name)?;
    let whole = captures.get(0)?;
    if whole.start() != 0 || whole.end() != asset.name.len() {
        log::debug!(
            "资产名不是整串匹配，忽略：地图={label}，资产={}",
            asset.name
        );
        return None;
    }
    let version = captures
        .name("version")
        .or_else(|| captures.get(1))?
        .as_str()
        .trim();
    (!version.is_empty()).then(|| version.to_string())
}

/// 追加一条版本；同一个版本号出现两次直接报错。
///
/// 版本号同时是「装哪个资产」的唯一标识：两个文件抽出同一个号，安装时只能
/// 瞎选一个，宁可在刷新阶段就明确报错。
fn push_version(
    versions: &mut Vec<(String, ReleaseAsset)>,
    version: String,
    asset: &ReleaseAsset,
    label: &str,
) -> Result<(), String> {
    if versions.iter().any(|(existing, _)| existing == &version) {
        return Err(crate::locale_tag::with_params(
            "mapSource.duplicateVersion",
            &[("map", label), ("version", &version)],
        ));
    }
    versions.push((version, asset.clone()));
    Ok(())
}

/// 版本号里的数字段，用于排序。
///
/// 只按数字段比：`0.10` > `0.9`，`1.7.0` > `1.7`。段数不一致是常态（作者手写
/// 的文件名），纯字符串比较会把 `0.10` 排到 `0.9` 前面，更错。
fn version_parts(version: &str) -> Vec<u64> {
    version
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

/// 新的排前面（降序）。
fn newest_first(left: &str, right: &str) -> std::cmp::Ordering {
    version_parts(right)
        .cmp(&version_parts(left))
        .then_with(|| right.cmp(left))
}
