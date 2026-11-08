use super::Infrastructure;

pub(super) const INFRASTRUCTURE: Infrastructure = Infrastructure {
    homepage: "https://gitee.com/",
    repository_api: "https://gitee.com/api/v5/repos/",
    hosts: &["gitee.com", "www.gitee.com", "raw.giteeusercontent.com"],
};
