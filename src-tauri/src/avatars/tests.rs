use super::*;

#[test]
fn avatar_cache_key_separates_providers_and_ignores_case() {
    let gh = cache_key(&Provider::Github, "BepInEx");
    let gh_lower = cache_key(&Provider::Github, "  bepinex  ");
    assert_eq!(gh, gh_lower, "owner 比较应忽略大小写与首尾空白");

    let gitee = cache_key(&Provider::Gitee, "BepInEx");
    assert_ne!(gh, gitee, "不同平台必须分开缓存");
}

/*
 * 返回给前端的键必须**小写化**，与 `avatars.ts::avatarKey()` 完全一致。
 *
 * 真实事故：后端曾用原样大小写的 owner 作键，前端用小写去查。
 * `CH3NGY2`、`QingyuQingxue` 这类含大写的 owner 于是查不到 ——
 * 日志显示「可用=8/8」，界面却只有 6 个头像，另外两个回落成平台图标。
 * 全小写的 owner 恰好一致，所以症状看起来像「某几个 owner 特别」。
 */
#[test]
fn response_key_is_lowercase_and_matches_frontend_rule() {
    assert_eq!(
        response_key(&Provider::Github, "CH3NGY2"),
        "github:ch3ngy2",
        "provider 前缀 + 小写 owner"
    );
    assert_eq!(
        response_key(&Provider::Github, "  QingyuQingxue  "),
        "github:qingyuqingxue",
        "首尾空白要去掉"
    );
    assert_eq!(response_key(&Provider::Gitee, "ch3ngyz"), "gitee:ch3ngyz");
    // 与前端 avatarKey() 的模板字符串 `${provider}:${owner.trim().toLowerCase()}` 同构
    for owner in ["BepInEx", "gua248", "CH3NGY2", "QingyuQingxue"] {
        let key = response_key(&Provider::Github, owner);
        assert_eq!(key, format!("github:{}", owner.to_lowercase()), "{owner}");
    }
}

/*
 * 头像缓存的有效期是 12 小时。这条规则有两面：
 *   · 太短 → 每次启动都重下几十张图，白费流量且首屏变慢；
 *   · 太长（或永久）→ 用户换了头像后界面一直显示旧图。
 * 用文件修改时间判定，所以测试直接把 mtime 拨到过去，而不是等 12 小时。
 */
#[test]
fn avatar_cache_expires_after_twelve_hours() {
    let dir = std::env::temp_dir().join(format!(
        "oc2-avatar-ttl-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    // `cache_path` 会再套一层 `avatars/`，测试里要先把这层建出来
    std::fs::create_dir_all(dir.join(AVATAR_DIR)).unwrap();
    assert_eq!(
        AVATAR_CACHE_TTL,
        std::time::Duration::from_secs(12 * 60 * 60)
    );

    let path = cache_path(&dir, &Provider::Github, "someone");
    std::fs::write(&path, b"\x89PNG\r\n\x1a\npayload").unwrap();
    assert!(cache_is_fresh(&path), "刚写入的文件必须算新鲜");
    assert!(
        read_cached(&dir, &Provider::Github, "someone").is_some(),
        "有效期内应直接命中磁盘缓存"
    );

    // 把修改时间拨到 TTL 之外 → 过期，read_cached 必须重新下载
    let stale =
        std::time::SystemTime::now() - AVATAR_CACHE_TTL - std::time::Duration::from_secs(60);
    let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_modified(stale).unwrap();
    drop(file);
    assert!(!cache_is_fresh(&path), "超过 12 小时必须判过期");
    assert!(
        read_cached(&dir, &Provider::Github, "someone").is_none(),
        "过期后不能再拿旧图当缓存"
    );
    assert!(path.is_file(), "过期只判未命中，不删文件");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn direct_url_uses_the_platform_owner_endpoint() {
    assert_eq!(
        direct_url(&Provider::Github, "BepInEx"),
        "https://github.com/BepInEx.png?size=64"
    );
    assert_eq!(
        direct_url(&Provider::Gitee, "ch3ngyz"),
        "https://gitee.com/ch3ngyz.png?size=64"
    );
}

/// 用户名里出现特殊字符时不能拼出畸形 URL，也不能逃出缓存目录。
#[test]
fn unsafe_owner_characters_are_encoded() {
    let url = direct_url(&Provider::Github, "a b/c?d");
    assert!(!url.contains(' '), "{url}");
    assert!(!url.contains("/c"), "斜杠应被编码：{url}");
    assert!(url.contains("%20"), "{url}");
}

#[test]
fn non_image_payloads_are_rejected() {
    assert!(validate(b"<html>rate limited</html>").is_err());
    assert!(validate(b"").is_err());
    assert!(validate(&vec![0u8; MAX_AVATAR_BYTES + 1]).is_err());
    // 真实图片头通过
    assert!(validate(&[0xFF, 0xD8, 0xFF, 0, 0]).is_ok());
    assert!(validate(b"\x89PNG\r\n\x1a\n").is_ok());
}

/*
 * 头像的代理候选**不能**按 raw 文件模式渲染。
 *
 * 真实事故：传 `is_raw_file = true` 时，`proxy_context` 会把目标 URL
 * 重写成 `raw.githubusercontent.com/{owner}/{repo}/{branch}/{path}`。
 * 头像地址是 `github.com/{owner}.png`，拆不出 repo，于是 owner 变成
 * `gua248.png`、repo 为空，最终渲染出
 * `https://raw.githubusercontent.com/gua248.png//main/` —— 所有代理
 * 一律 403/404，每个头像都要白试好几轮。
 *
 * 断言：非 raw 模式下，渲染结果里不得出现被改写成 raw 路径的形态。
 */
#[test]
fn avatar_candidates_are_not_rewritten_into_raw_paths() {
    let proxy = |id: &str, template: &str| crate::models::GitHubProxy {
        id: id.into(),
        name: id.into(),
        template: template.into(),
        regions: Vec::new(),
        description: String::new(),
        enabled: true,
        builtin: true,
        remote: false,
    };
    let proxies = vec![
        proxy("wrap", "https://wrap.example/{url}"),
        // 依赖 owner/repo/branch/path 的模板：raw 模式下会被喂进错误的分段
        proxy(
            "rawstyle",
            "https://rawstyle.example/{owner}/{repo}/{branch}/{path}",
        ),
    ];
    let url = direct_url(&Provider::Github, "gua248");

    // 错误的写法：raw 模式会把 .png 当成 owner 并拼出空 repo/分支
    let raw_mode = crate::download::planning::asset_proxy_urls(
        &proxies,
        &url,
        true,
        "",
        "",
        crate::models::ProxyMode::AutoProxy,
        None,
    );
    assert!(
        raw_mode
            .iter()
            .any(|candidate| candidate.contains("raw.githubusercontent.com")
                || candidate.contains("//main")),
        "raw 模式应当能复现畸形地址，否则这条测试没有意义：{raw_mode:?}"
    );

    // 正确写法：非 raw 模式，不得出现被改写或空段拼接的地址
    let avatar_mode = crate::download::planning::asset_proxy_urls(
        &proxies,
        &url,
        false,
        "",
        "",
        crate::models::ProxyMode::AutoProxy,
        None,
    );
    for candidate in &avatar_mode {
        assert!(
            !candidate.contains("raw.githubusercontent.com"),
            "不应被改写成 raw 路径：{candidate}"
        );
        assert!(!candidate.contains("//main"), "不应拼出空分支：{candidate}");
    }
    assert!(
        avatar_mode
            .iter()
            .any(|candidate| candidate
                == "https://wrap.example/https://github.com/gua248.png?size=64"),
        "纯包装模板必须原样渲染：{avatar_mode:?}"
    );
}

/*
 * 头像必须读 `avatar_proxies`，不能读 raw 组。
 *
 * 五类代理各有自己的列表（release / raw / avatar / github_api /
 * gitee_api）。头像 URL 是 `github.com/{owner}.png`，raw 组的模板都按
 * 仓库文件路径设计，实测全部取不到；混用会让每个头像白试整条 raw 列表。
 *
 * 头像代理的模板只需满足「能把整条 URL 或其路径转给上游」之一：
 *   `{url}` / `{url_no_scheme}` —— 纯包装型
 *   `{url_path}` —— 只转路径型（如 oc2gh，它按 Host 推断上游）
 * 其余占位符（owner/repo/branch/path）无法表达头像地址，会拼出空段。
 */
#[test]
fn bundled_avatar_proxies_exist_and_can_serve_any_url() {
    let bundled = crate::github_proxies::bundled_defaults();
    assert!(
        !bundled.avatar_proxies.is_empty(),
        "内置清单应当声明头像代理"
    );
    for proxy in &bundled.avatar_proxies {
        let wrappable = ["{url}", "{url_no_scheme}", "{url_path}"]
            .iter()
            .any(|placeholder| proxy.template.contains(placeholder));
        assert!(
            wrappable,
            "头像代理必须能服务任意 URL（含 {{url}}/{{url_no_scheme}}/{{url_path}}）：{} = {}",
            proxy.id, proxy.template
        );
        // 仓库路径类占位符对头像是无意义的：头像 URL 没有这些分段
        for forbidden in ["{owner}", "{repo}", "{branch}", "{path}", "{github_path}"] {
            assert!(
                !proxy.template.contains(forbidden),
                "头像代理不应依赖仓库路径占位符 {forbidden}：{} = {}",
                proxy.id,
                proxy.template
            );
        }
        assert!(proxy.builtin, "内置头像代理应标记 builtin：{}", proxy.id);
    }
    // 与另外几组分开维护：不应与 raw 组共用同一批 id
    let raw_ids: std::collections::HashSet<&str> = bundled
        .raw_proxies
        .iter()
        .map(|proxy| proxy.id.as_str())
        .collect();
    for proxy in &bundled.avatar_proxies {
        assert!(
            !raw_ids.contains(proxy.id.as_str()),
            "头像代理不应当与 raw 代理共用 id：{}",
            proxy.id
        );
    }
}
