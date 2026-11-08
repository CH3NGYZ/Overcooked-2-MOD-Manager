use super::Infrastructure;

pub(super) const INFRASTRUCTURE: Infrastructure = Infrastructure {
    homepage: "https://github.com/",
    repository_api: "https://api.github.com/repos/",
    hosts: &["github.com", "www.github.com", "raw.githubusercontent.com"],
};
