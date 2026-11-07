//! 地图发布的授权、会话、官方请求、仓库、Release 与 ZIP 各自持有真实实现。
//! 读取/渲染声明块复用 map_note；本地 ZIP 解析复用 level_sets。

pub(crate) mod assets;
pub(crate) mod authorization;
mod credentials;
mod http;
pub(crate) mod releases;
pub(crate) mod repositories;
pub(crate) mod session;
mod validation;
pub(crate) mod zip;
