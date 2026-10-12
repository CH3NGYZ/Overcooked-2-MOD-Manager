# 跨平台运行、更新与验收

本文件记录管理器的运行模式、用户数据和签名发布约定，并列出真实平台的验收项。配置完成、本地编译和测试通过，都不能代替真实 CI、安装器、Steam 或游戏运行验证。

## 用户数据与运行模式

所有平台的数据根均为当前用户的 `~/OC2ModManager`，其中保存 `config.toml`、缓存、安装清单、回收站、框架备份、安装语言及日志。本轮按全新安装处理，不读取、导入或删除旧安装位置的数据。以后更新保留新的用户目录及已经选择的运行模式。

| 平台 | 数据目录 | 管理器安装位置 / 默认游戏模式 |
| --- | --- | --- |
| Windows | `C:/Users/<user>/OC2ModManager` | 程序仍默认装在 `%LOCALAPPDATA%/OC2ModManager`；Windows 游戏，仅保留现有教程 |
| Linux | `/home/<user>/OC2ModManager`；root 为 `/root/OC2ModManager` | 原生 Linux 管理器；厨房默认 Wine，可在厨房目录卡片切换原生模式 |
| macOS | `/Users/<user>/OC2ModManager` | 原生 macOS 管理器；厨房默认 Wine/CrossOver，可切换原生模式 |

切换模式清空已选择的游戏路径并重新检测，不移动游戏或 MOD。游戏运行、操作繁忙或教程演示时不可切换。Windows/Wine 游戏是 x86，和管理器自身的 x64 / ARM64 架构无关。

| 游戏模式 | Steam / 游戏 | BepInEx 5.4.23.5 包 |
| --- | --- | --- |
| Windows，以及 Linux/macOS Wine | Windows Steam、`Overcooked2.exe` | `BepInEx_win_x86_5.4.23.5.zip` |
| Linux 原生 | 原生 Linux Steam、`Overcooked2.x86_64` | `BepInEx_linux_x64_5.4.23.5.zip` |
| macOS 原生 | 原生 macOS Steam、`Overcooked! 2.app` 所在目录 | `BepInEx_macos_universal_5.4.23.5.zip` |

GitHub 包来自 `BepInEx/BepInEx` 的 `v5.4.23.5`；Gitee 包来自 `ch3ngyz/Overcooked-2-Installer-ResourceFiles` 的 `bepinex` Release，三个包均保留原名，不再使用 `bepinex.zip` 别名。下载使用现有取消、竞速和 SHA-256 校验；重装保留游戏的 MOD 与配置，脚本校验失败回滚文件及本次新建的空目录。

## Steam 与 Wine 的安装后设置

- 原生安装自动设置 `run_bepinex.sh` 的 `executable_name` 及用户执行权限；框架开关修改脚本的加载开关，不重命名原生库。
- Linux 原生 Steam 启动项为 `./run_bepinex.sh %command%`；macOS 原生使用 `"<游戏根目录>/run_bepinex.sh" %command%`，实际路径由界面生成并转义。原生模式不要强制使用 Proton。
- 原生厨房启动明确选择对应 Steam：Linux 使用原生 Steam 或 Flatpak 入口，Mac 使用 `com.valvesoftware.steam` 的应用身份，不交给 Wine 注册的协议处理器或同名 CrossOver 导出启动器。
- Wine 只探测目标 prefix 内的 Windows Steam，并转换其 VDF 中的 Windows 库路径；macOS 使用 CrossOver Bottles，Linux 使用 `WINEPREFIX` 或 `~/.wine`。外置 Wine 库按该 prefix 的 `dosdevices` 映射解析。
- Wine 框架安装后尝试在同一 prefix 中设置 `winhttp=native,builtin`；失败不伪报为成功，界面显示手工 winecfg 指引。已运行的 Steam 需要退出后重启。Linux 缺文字时使用目标 prefix 的 `winetricks corefonts`，Mac 则在同一 CrossOver 容器安装 Core Fonts。
- Windows 原有教程不变；Mac CrossOver 教程只在 Wine 模式显示。完整 Linux 教程后续制作，本轮仅提供安装后的必要启动设置。

官方参考：[原生 Steam 设置](https://docs.bepinex.dev/articles/advanced/steam_interop.html)、[Proton/Wine 设置](https://docs.bepinex.dev/articles/advanced/proton_wine.html)。

## 四个构建目标、七种更新格式

| 构建目标 | 安装包 | 自动更新包 / 清单键 |
| --- | --- | --- |
| Windows x64 | `OC2ModManager_<版本>_x64-setup.exe` | 同一 EXE；`windows-x86_64-nsis` |
| Windows x64 | `OC2ModManager_<版本>_x64_en-US.msi` | 同一 MSI；`windows-x86_64-msi` |
| Linux x64 | `OC2ModManager_<版本>_amd64.AppImage` | 同一 AppImage；`linux-x86_64-appimage` |
| Linux x64 | `OC2ModManager_<版本>_amd64.deb` | 同一 DEB；`linux-x86_64-deb` |
| Linux x64 | `OC2ModManager-<版本>-1.x86_64.rpm` | 同一 RPM；`linux-x86_64-rpm` |
| Mac Apple Silicon | `OC2ModManager_<版本>_aarch64.dmg` | `OC2ModManager_aarch64.app.tar.gz`；`darwin-aarch64-app` |
| Mac Intel | `OC2ModManager_<版本>_x64.dmg` | `OC2ModManager_x64.app.tar.gz`；`darwin-x86_64-app` |

RPM 文件名为 `名称-版本-Release.架构.rpm`。`-1` 是 RPM 的包修订号，来自 Tauri `bundle.linux.rpm.release` 的默认值 `"1"`，不是应用 Beta 号或 SemVer 的一部分；应用版本仍是 `1.2.0`。同一应用版本重新打包可以增加 RPM 修订号，但管理器自更新仍按应用版本判断是否有更新。

客户端按当前系统、进程架构和 Tauri 写入的安装格式标记选包，不能跨架构或跨 DEB/RPM/AppImage 安装。Mac 自动更新使用应用归档，不直接运行 DMG；Intel Mac 与 Apple Silicon 分开。Windows 保留原更新助手、提权及原位安装；错误的 plugins 安装位置只迁移管理器程序，不迁移旧配置。Linux/macOS 安装成功后由原生 updater 重启；DEB/RPM 更新需要系统授权，AppImage/应用包目标需要可写权限。

当前锁定的 `tauri-plugin-updater 2.13.1` 支持 AppImage/DEB/RPM 和 Mac 应用归档。自定义 `install(bytes)` 不负责验签，因此现有竞速下载在每个候选胜出前校验 minisign 签名与受签名保护的版本，GitHub 提供 SHA-256 时同时校验摘要。缺失签名、版本不符、错误的包名或格式均停止安装，不降级为无校验更新。

## 发布前置条件

`.github/workflows/build.yml` 使用 Windows、Ubuntu 22.04、Mac Apple Silicon 和 `macos-15-intel` 四个 runner。所有构建上传到 `CH3NGYZ/Overcooked-2-MOD-Manager-Releases` 的同一草稿；在四个构建成功后，由独立任务验证实际上传字节和签名，再汇总七目标 `latest.json`。并行构建不各自上传清单，也不自动发布草稿。

代码仓库 Actions 必须配置以下 Secrets：

- `RELEASE_REPOSITORY_TOKEN`：对 Releases 仓库有 Contents 写权限；代码仓库默认的 `GITHUB_TOKEN` 不能代替跨仓库权限。
- `TAURI_SIGNING_PRIVATE_KEY`：与 `src-tauri/tauri.conf.json` 的已有 updater 公钥对应，不能为绕过失败随意换一把密钥。
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：私钥有密码时配置。

这些是 updater 的 minisign 密钥，不是 Apple 开发者证书或 Windows 代码签名证书。非发布打包测试使用 CI 临时密钥即可；正式更新必须使用长期保管、与客户端公钥匹配的私钥，操作系统代码签名和 macOS 公证需单独配置与验收。

正式草稿工作流由 `v<项目版本>` 标签或手动 dispatch 的 `mode=release` 触发；项目的 package、Tauri、Cargo 及两个 lock 文件版本必须一致。发布 dispatch 使用项目版本生成标签。已公开的同名 Release 不允许覆盖。`scripts/build-release.mjs` 对包括 DEB/RPM 在内的更新包补签并检查公钥；聚合任务检查缺包、重复包、错误下载 URL、摘要、签名与版本后才上传清单。

已有的未签名包、仅有 DMG 的 Release 或缺少对应平台键的清单，不会因为源码修改而自动具备更新能力。需要在授权发布后上传完整更新包、各自 `.sig` 与 `latest.json`；Gitee 镜像必须同步这些原始字节，不重新压缩、改名或只同步安装包。测试分支仅在用户明确授权后推送；公开发布及 Gitee 同步仍不执行。

## 非发布四端 Actions 打包测试

手动运行已有 `build.yml`，选择测试分支和 `mode=test`（默认值），会运行同一四端矩阵，但跳过所有 Release 准备、上传和清单发布步骤。测试只需要仓库的 Actions 权限，不需要跨仓库发布 Token 或生产签名 Secret；不得将旧提交上的三端成功记录视为新实现的四端结果。

`scripts/ci-packages.mjs` 在明确的测试模式下生成临时 updater 密钥，捕获生成器全部输出，不在日志或产物中保存私钥。它只在 runner 的临时检出中替换公钥并禁用正式更新地址，再使用同一 `build-release.mjs` 打包和验签。私钥通过 `TAURI_SIGNING_PRIVATE_KEY_PATH` 的文件路径传递，任务结束时删除；共用 CLI 调用边界仅为 `tauri build` 映射为其接受的 `TAURI_SIGNING_PRIVATE_KEY` 路径，`signer sign` 仍使用 `TAURI_SIGNING_PRIVATE_KEY_PATH`，不能把路径误当成内联密钥内容。映射不修改父进程环境，也不读取或输出私钥内容。

每个平台上传 `package-test-<Rust target>`，包含包、签名以及标明 `testOnly` 的版本/提交/摘要报告。两个 Mac DMG 加上七种更新格式共九个包；独立聚合任务重新下载并核对四个构建目标、同一提交、字节摘要及七种签名，结果保存为 `four-platform-package-summary`。构建日志另存 `package-build-log-<Rust target>`。这些 Actions 产物保留七天，不创建或修改 Release，不执行安装器或游戏。

临时测试签名仅证明打包和校验调用链可以执行，不证明生产 Secret 与公钥匹配，也不证明 Apple 签名、公证、系统权限或实际自动更新成功。测试包不能作为正式更新发布。

## 本地可执行回归

从仓库根运行：

```powershell
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml --locked --jobs 1
cargo test --manifest-path src-tauri/Cargo.toml --lib --locked --jobs 1
git diff --check
```

测试运行前把 `TEMP`、`TMP`、`TMPDIR` 指向工作区已忽略的 `src-tauri/target/verification/<本次记录>/tmp`，避免测试临时文件占用系统临时目录。

跨平台安装回归不能依赖 `fs::copy` 拒绝自复制：Unix 可能成功返回，Windows 的失败回滚也可能改变硬链接源的权限。`copy_asset_pending` 在创建快照或修改权限前比较实际文件身份；同一路径、路径别名及硬链接均拒绝覆盖自身。回收站链接边界测试分别使用 `remove_dir` 删除 Windows 目录联接、`remove_file` 删除 Unix 符号链接，只删除链接本身并检查链接外文件保持不变。

下载进度轮换测试使用缩短的 40 ms 窗口，不能依赖 Windows 调度器每 2 ms 必定执行。测试在候选工厂同步准备每阶段的 reporter 快照，并保留超过窗口的受控调度延迟、领先源字节及总大小断言；不改动生产竞速窗口或下载逻辑。

将三个官方 ZIP 放到工作区的已忽略目录后，可显式执行真实包安装/重装测试（不启动游戏或完整安装器）：

```powershell
$env:OC2_BEPINEX_PACKAGE_DIR = (Resolve-Path 'src-tauri/target/verification/platform-packages').Path
cargo test --manifest-path src-tauri/Cargo.toml --lib --locked --jobs 1 commands::framework_install_tests::installs_the_actual_platform_archives_with_digest_and_preservation_checks -- --ignored --exact
```

只安装 Rust target 不能在 Windows 上证明 Linux/macOS 完整构建成功；原生 UI 依赖、链接器和 SDK 必须在对应操作系统准备。Linux 构建依赖以工作流 apt 安装列表为准，Mac 需要 Xcode Command Line Tools。不要用会安装并启动应用的 `build.bat` 代替静态验证。

## 真实平台验收清单

1. 在四个构建环境分别确认格式、编译、完整 Rust 回归及签名产物；检查草稿清单包含七个键，未签名和坏签名包均无法安装。
2. 每种安装格式从较旧版本更新，检查只选择本格式/架构、权限失败可重试、取消保留旧程序、安装失败有提示，以及成功后版本与重启结果。
3. 确认用户目录产生配置、日志、清单和备份；切换安装位置或整包更新不覆盖新数据根；旧安装目录数据不被读取或删除。
4. Linux/macOS 分别验证默认 Wine、切换原生、重启后模式持久化、对应 Steam 探测、外置库、游戏运行检测和框架开关；双模式同时存在时不能串用路径或 prefix。
5. 原生安装后通过 Steam 启动并检查 `BepInEx/config/BepInEx.cfg`、`BepInEx/LogOutput.log`；Wine 验证目标容器 winhttp 覆盖、已有 Steam 重启、MOD UI 字体及 Mac CrossOver 向导。

本轮 Windows 主机的编译、单元回归和官方三包的文件系统验证，不替代上述 CI、Linux/Mac 实机安装更新及实际游戏加载验收。
