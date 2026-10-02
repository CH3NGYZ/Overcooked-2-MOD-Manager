# OC2 Mod Manager

基于 Rust、Tauri 2、Vue 3 和 TypeScript 的《胡闹厨房 2》Windows x64 MOD 管理器。程序直接由 Rust/Tauri 运行，不依赖 Python、.NET、WPF 或旧版安装器。

## 开发环境

Rust 工具链位于 `D:\ProgramData\rust`，构建前设置：

```powershell
$env:CARGO_HOME = 'D:\ProgramData\rust\.cargo'
$env:RUSTUP_HOME = 'D:\ProgramData\rust\.rustup'
```

需要 Rust MSVC toolchain、Windows SDK、Node.js 18+ 和 WebView2 Runtime。Rust 命令使用 `x86_64-pc-windows-msvc` 目标。

## 开发与测试

```powershell
npm install
npm run build

$env:CARGO_HOME = 'D:\ProgramData\rust\.cargo'
$env:RUSTUP_HOME = 'D:\ProgramData\rust\.rustup'
& 'D:\ProgramData\rust\.cargo\bin\cargo.exe' fmt --manifest-path src-tauri/Cargo.toml -- --check
& 'D:\ProgramData\rust\.cargo\bin\cargo.exe' test --manifest-path src-tauri/Cargo.toml -j 1
npx tauri dev
```

`cargo test` 覆盖 URL/Provider、版本和频道、资产规则、代理重写、TOML 配置迁移和导出、导入冲突、ZIP Slip、SHA-256、安装清单、进程保护和 `winhttp.dll` 状态机。

## 配置与安全

内部配置位于 `%LOCALAPPDATA%\OC2ModManager\config.toml`，使用 TOML 原子写入；损坏配置会先重命名为时间戳备份，再生成默认配置。Token 使用 Windows DPAPI 加密，公共配置、仓库导出和日志不包含 Token、密文、游戏绝对路径或本地清单。

网络请求统一使用 `Overcooked 2 Mod Manager/<版本> (client=desktop; os=Windows; arch=x64; tauri=2)` User-Agent，便于 Cloudflare 按客户端和版本放行。GitHub API 默认使用 `https://oc2ghapi.ch3ng.top`，Gitee API 默认使用 `https://oc2gtapi.ch3ng.top`，两者按 Provider 独立选择；Release 下载代理仍使用 `https://oc2gh.ch3ng.top`。

仓库交换格式使用 TOML，文件扩展名为 `.oc2repo.toml`。导出只包含仓库设置，导入会校验格式、版本、URL、Provider、资产规则和安全安装目录，并提供跳过、覆盖和保留双方三种冲突策略。

所有修改游戏目录的命令都会匹配完整的 `Overcooked2.exe` 路径并检查厨房进程。MOD 安装使用临时下载文件、SHA-256 校验、Zip Slip 防护和安装清单；厨房在下载期间重新启动时会取消后续写入。

BepInEx 和 MOD 版本优先读取 Windows PE `FileVersion`。MOD 文件版本为 `0.0.0` 或 `0.0.0.0` 时，才回退读取 DLL 中的 `BepInPlugin` 版本参数。

“MOD 库”页面会读取 DLL 的 `ProductName` 和 `OriginalFilename`，因此玩家重命名 DLL 后仍能识别来源。备注通过产品 GUID（没有 GUID 时使用原始文件名）保存到配置，可直接说明 MOD 的用途；同 GUID 存在多个版本时，较低版本显示“存在更高版本，框架默认不加载”。

## 构建发行包

```powershell
$env:CARGO_HOME = 'D:\ProgramData\rust\.cargo'
$env:RUSTUP_HOME = 'D:\ProgramData\rust\.rustup'
npx tauri build
```

构建产物位于 `src-tauri/target/release` 和 `src-tauri/target/release/bundle`。Tauri 配置启用 Windows bundle，正式发布前需要在构建机安装 WiX/NSIS 工具和 WebView2 Bootstrapper，并配置 Tauri updater 的签名公钥与发布端点。

## 运行说明

首次启动会创建默认仓库并按配置、当前目录、Steam 注册表及 `libraryfolders.vdf` 顺序查找厨房目录。若未找到目录，请在配置中填写包含 `Overcooked2.exe` 的目录。启动厨房时默认通过 Steam URI 请求启动，并保留手动确认流程。
