fn main() {
    embed_repository_sources();
    embed_github_app_secret();
    embed_gitcode_oauth();
    link_windows_test_manifest();
    tauri_build::build()
}

fn embed_gitcode_oauth() {
    let mut generated = String::new();
    for name in ["CLIENT_ID", "CLIENT_SECRET", "SCOPE"] {
        let variable = format!("OC2_GITCODE_OAUTH_{name}");
        println!("cargo:rerun-if-env-changed={variable}");
        // Scope must be chosen when registering the application; do not silently
        // request undocumented narrow permissions or all account permissions.
        let value = std::env::var(variable).unwrap_or_default();
        generated.push_str(&format!(
            "pub const GITCODE_OAUTH_{name}: &str = {:?};\n",
            value.trim()
        ));
    }
    let output =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("gitcode_oauth.rs");
    std::fs::write(output, generated).expect("无法写入 GitCode OAuth 配置");
}

/// GitHub App 的 Client Secret 不进仓库：优先读环境变量（CI 用 secret），
/// 其次读被忽略的 `.tmp/github-app-secret.txt`（本机构建）。两处都没有时注入
/// 空串，上传页会在换 Token 时报 `mapPublish.missingClientSecret`，不会静默失败。
fn embed_github_app_secret() {
    let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let local = manifest
        .join("..")
        .join(".tmp")
        .join("github-app-secret.txt");
    println!("cargo:rerun-if-env-changed=OC2_GITHUB_APP_CLIENT_SECRET");
    println!("cargo:rerun-if-changed={}", local.display());
    let secret = std::env::var("OC2_GITHUB_APP_CLIENT_SECRET")
        .ok()
        .or_else(|| std::fs::read_to_string(&local).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    let output =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("github_app.rs");
    let generated = format!("pub const GITHUB_APP_CLIENT_SECRET: &str = {secret:?};\n");
    std::fs::write(output, generated).expect("无法写入 GitHub App 凭据");
}

fn link_windows_test_manifest() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc")
    {
        return;
    }
    // Tauri embeds Common Controls v6 only into app binaries, not test executables.
    let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("tests/windows-webview.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
        manifest.display()
    );
}

fn embed_repository_sources() {
    let root =
        std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("resources");
    let mut pending = vec![root.clone()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        println!("cargo:rerun-if-changed={}", directory.display());
        for entry in std::fs::read_dir(directory).expect("无法读取内置仓库源目录") {
            let entry = entry.expect("无法读取内置仓库源文件");
            let file_type = entry.file_type().expect("无法读取内置仓库源文件类型");
            let path = entry.path();
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension == "toml")
            {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut generated = String::from("const BUNDLED_SOURCES: &[(&str, &str)] = &[\n");
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let relative = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        generated.push_str(&format!(
            "    ({relative:?}, include_str!({:?})),\n",
            path.to_string_lossy()
        ));
    }
    generated.push_str("];\n");
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
        .join("repository_sources.rs");
    std::fs::write(output, generated).expect("无法打包内置仓库源");
}
