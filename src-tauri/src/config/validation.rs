//! 仓库定义和关系版本范围校验。

use super::ConfigError;
use super::availability::validate_repository_platform;
use super::availability::validate_repository_region;
use crate::models::*;
use std::path::Path;

pub(crate) fn validate_repository(repo: &RepositoryExport) -> Result<(), ConfigError> {
    validate_repository_region(repo.region.as_deref())?;
    validate_repository_platform(repo.platform.as_deref(), &repo.name)?;
    crate::releases::repository_coordinates(&repo.username, &repo.repository).map_err(|error| {
        ConfigError::Invalid(crate::locale_tag::with_params(
            "config.homepageInvalid",
            &[("repo", &repo.name), ("reason", &error.to_string())],
        ))
    })?;
    if Path::new(&repo.install_target).is_absolute()
        || repo
            .install_target
            .split(['/', '\\'])
            .any(|part| part == "..")
    {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "config.installTargetUnsafe",
            &[("repo", &repo.name.to_string())],
        )));
    }
    match repo.download_mode {
        RepositoryDownloadMode::Release => {
            if repo.asset_rules.is_empty()
                || repo
                    .asset_rules
                    .iter()
                    .any(|rule| rule.value.trim().is_empty())
            {
                return Err(ConfigError::Invalid("config.assetRuleRequired".into()));
            }
        }
        RepositoryDownloadMode::RepositoryFile => {
            crate::releases::repository_file_url(
                &repo.provider,
                &repo.username,
                &repo.repository,
                &repo.branch,
                &repo.file_path,
            )
            .map_err(|error| ConfigError::Invalid(error.to_string()))?;
            if repo
                .asset_rules
                .iter()
                .any(|rule| rule.rule_type != AssetRuleType::Exact)
            {
                return Err(ConfigError::Invalid(
                    "config.fileModeExactAssetsOnly".into(),
                ));
            }
        }
    }
    if !repo.version_regex.trim().is_empty()
        && regex::Regex::new(repo.version_regex.trim()).is_err()
    {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "config.versionRegexInvalid",
            &[("repo", &repo.name.to_string())],
        )));
    }
    /*
     * 关系声明校验。
     *
     * GUID 是关系的唯一标识，空值或纯空白的声明永远匹配不上任何插件，
     * 等于一条静默失效的配置 —— 在导入时就拒绝，比等到界面上不提示更容易发现。
     * `version` 若填写，必须是可解析且非空的区间；
     * 否则「是否满足」永远判不出来。
     */
    for relation in &repo.relations {
        validate_relation(relation, &repo.name, &repo.plugin_guid)?;
    }
    /*
     * 按 Release 覆盖的关系表也要校验，且区间不能为空。
     *
     * 不写区间意味着「匹配任何版本」，那和在 `relations` 里直接写没有区别，
     * 但作者会以为自己限定了范围。在导入阶段拒绝，比等它悄悄生效好。
     */
    for override_entry in &repo.release_relations {
        validate_release_override(&override_entry.version, &repo.name)?;
        for relation in &override_entry.relations {
            validate_relation(relation, &repo.name, &repo.plugin_guid)?;
        }
    }
    Ok(())
}

/// 单条关系声明的校验：GUID 必填，版本边界若填写必须可解析。
///
/// `optional` 只对 `requires` 有意义 —— 「可选的冲突」自相矛盾，
/// 写出来只会让前端不知道该怎么提示，所以在导入阶段就拒绝。
/// `prerelease` 反过来只对冲突/兼容有意义：依赖要求「必须是测试版」
/// 不是作者会想表达的事，具体版本要求应写在 `version` 区间里。
///
/// **指向自己 GUID 的冲突/兼容必须写 `prerelease`**：那是「正式版与测试版
/// 互斥」的表达方式（两条发布线同 GUID）。不写标志时它等价于「我不能和
/// 自己共存」，永远为真 —— 这种配置一定写错了，导入阶段就拒绝。
fn validate_relation(
    relation: &RepositoryRelation,
    repository_name: &str,
    owner_guid: &str,
) -> Result<(), ConfigError> {
    let kind = relation.kind.as_str();
    if relation.guid.trim().is_empty() {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "config.relationMissingGuid",
            &[("repo", repository_name), ("kind", kind)],
        )));
    }
    if relation.optional && relation.kind != RelationKind::Requires {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "config.relationOptionalUnsupported",
            &[
                ("repo", repository_name),
                ("kind", kind),
                ("guid", relation.guid.trim()),
            ],
        )));
    }
    if relation.prerelease.is_some() && relation.kind == RelationKind::Requires {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "config.relationRequiresPrerelease",
            &[("repo", repository_name), ("guid", relation.guid.trim())],
        )));
    }
    if relation.kind != RelationKind::Requires
        && relation.prerelease.is_none()
        && !owner_guid.trim().is_empty()
        && relation.guid.trim().eq_ignore_ascii_case(owner_guid.trim())
    {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "config.relationSelfGuid",
            &[("repo", repository_name), ("kind", kind)],
        )));
    }
    if let Err(problem) = check_version_spec(relation.version.trim()) {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            match problem {
                // 写反的区间（`>=2.0,<1.0`）永远命中不到版本，报「区间无效」
                VersionSpecProblem::EmptyInterval => "config.relationRangeInvalid",
                VersionSpecProblem::Unparsable => "config.relationValueInvalid",
            },
            &[
                ("repo", repository_name),
                ("kind", kind),
                ("guid", relation.guid.trim()),
                ("value", relation.version.trim()),
            ],
        )));
    }
    Ok(())
}

/// 版本区间校验的两种失败：读不懂，或者读懂了但永远匹配不到版本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VersionSpecProblem {
    Unparsable,
    EmptyInterval,
}

/// 解析并验证一段版本区间；空串表示不限版本，合法。
///
/// 抽成一处是因为关系条目与按版本覆盖表要同一条判据，只是提示文案不同。
pub(crate) fn check_version_spec(spec: &str) -> Result<(), VersionSpecProblem> {
    let range = crate::version_range::VersionRange::parse(spec)
        .map_err(|_| VersionSpecProblem::Unparsable)?;
    if range.is_empty_interval(crate::releases::compare_release_versions) {
        return Err(VersionSpecProblem::EmptyInterval);
    }
    Ok(())
}

/// 覆盖表的版本区间：必须解析得出来，并且不能是「不限」。
///
/// 不写区间等于让它匹配任何版本，效果与直接改 `relations` 相同，但作者
/// 会以为自己限定了范围 —— 这种「以为写了对配置」的错误在导入阶段拒绝。
fn validate_release_override(spec: &str, repository_name: &str) -> Result<(), ConfigError> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "config.relationsEmpty",
            &[("repo", repository_name)],
        )));
    }
    match check_version_spec(spec) {
        Ok(()) => Ok(()),
        Err(problem) => Err(ConfigError::Invalid(crate::locale_tag::with_params(
            match problem {
                VersionSpecProblem::EmptyInterval => "config.relationsRangeInvalid",
                VersionSpecProblem::Unparsable => "config.relationsValueInvalid",
            },
            &[("repo", repository_name), ("value", spec)],
        ))),
    }
}
