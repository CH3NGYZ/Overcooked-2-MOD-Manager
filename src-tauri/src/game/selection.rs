use crate::bepinex::Package;
use crate::models::GameConfig;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn game_executable_exists(game_dir: &Path, settings: &GameConfig) -> bool {
    executable_exists_for_package(game_dir, Package::current(settings))
}

fn executable_exists_for_package(game_dir: &Path, package: Package) -> bool {
    let executable = game_dir.join(package.executable());
    if package == Package::MacOS {
        executable.is_dir() && executable.join("Contents/MacOS").is_dir()
    } else {
        executable.is_file()
    }
}

pub(crate) fn selected_game_root(
    selected: &Path,
    settings: &GameConfig,
) -> Result<PathBuf, String> {
    selected_root_for_package(selected, Package::current(settings))
}

fn selected_root_for_package(selected: &Path, package: Package) -> Result<PathBuf, String> {
    let root = selection_root(selected, package);
    if !executable_exists_for_package(&root, package) {
        return Err(match package {
            Package::Windows => "game.exeNotFound",
            Package::MacOS => "game.appNotFound",
        }
        .into());
    }
    fs::canonicalize(&root).map_err(|_| "game.invalidDirectory".into())
}

/// 用户选中的路径 → 游戏根目录候选（纯路径运算，不访问磁盘）。
///
/// macOS 允许直接选游戏根、`Overcooked2.app` 包、包内的 `Contents/MacOS`
/// 甚至其中的可执行文件：后三种都向上回溯到包含 `.app` 的那一层，和自动
/// 探测得到同一个根字符串。Windows 包只接受游戏根。
pub(super) fn selection_root(selected: &Path, package: Package) -> PathBuf {
    let selected = expand_home(selected);
    if package != Package::MacOS {
        return selected;
    }
    // 向上找 `.app` 组件本身（不是只比较最后一段），`Contents/MacOS` 也能覆盖。
    let Some(app) = selected.ancestors().find(|candidate| {
        candidate
            .file_name()
            .is_some_and(|name| name == package.executable())
    }) else {
        return selected;
    };
    app.parent().map(Path::to_path_buf).unwrap_or(selected)
}

/// 展开配置/选择器里的 `~`。
///
/// 只认开头的 `~` 或 `~/`、`~\`；`~user` 需要真实账户查询，不做猜测，原样返回。
fn expand_home(path: &Path) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path.to_path_buf();
    };
    let rest = match text {
        "~" => "",
        value if value.starts_with("~/") || value.starts_with("~\\") => &value[1..],
        _ => return path.to_path_buf(),
    };
    let Some(home) = dirs::home_dir() else {
        return path.to_path_buf();
    };
    home.join(rest.trim_start_matches(['/', '\\']))
}

#[cfg(test)]
mod tests;
