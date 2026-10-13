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

RPM 文件名为 `名称-版本-Release.架构.rpm`。`-1` 是 RPM 的包修订号，来自 Tauri `bundle.linux.rpm.release` 的默认值 `"1"`，不是应用 Beta 号或 SemVer 的一部分；例如 `OC2ModManager-1.2.1-1.x86_64.rpm` 的应用版本仍是 `1.2.1`。同一应用版本重新打包可以增加 RPM 修订号，但管理器自更新仍按应用版本判断是否有更新。

只构建 Windows、Linux 和 macOS 桌面应用，不配置 Android/iOS。Mac Intel 与 Apple Silicon 是同一桌面系统的两个构建目标，因此 Actions 仍有四个矩阵任务。

三个桌面系统统一通过 `tauri-plugin-updater` 解析 `latest.json` 并调用 `Update::install`。客户端按当前系统、进程架构和 Tauri 写入的安装格式标记选包，不能跨架构或跨 DEB/RPM/AppImage 安装。Mac 自动更新使用应用归档，不直接运行 DMG；Intel Mac 与 Apple Silicon 分开。Linux/macOS 安装完成后调用 Tauri 重启；DEB/RPM 更新需要系统授权，AppImage/应用包目标需要可写权限。

Windows 保留启动与安装器提权、正常位置原位更新，但不再复制或启动 `OC2UpdateHost.exe`。Updater 自带 `/S /UPDATE` 或 MSI 静默参数，应用追加自己的成功启动标记及安装目录，并关闭 Updater 默认的第二套重启参数。NSIS 的 `/D=目录` 必须最后且不加引号；MSI 的目录及日志参数显式引用。危险位置的 NSIS 仍由安装器校验后迁移；MSI 显式传递受保护的 `OC2MIGRATEFROM`，包括目录联接别名，迁移前校验实际路径。迁移仅处理管理器程序，保留旧配置及其他 MOD。

Windows 的 `Update::install` 在启动安装器成功后退出进程，启动失败则返回错误并保留当前程序。正常更新不再由外部助手监测安装器退出码、落盘版本与新进程；安装器只在成功钩子启动新程序，MSI 详细日志保存在用户目录的 `logs/update-msi.log`。不能将“已启动安装器”说成“已完成更新”。危险位置的安装器迁移仍核对落盘版本、确认新进程持续运行后按快照清理原程序。

当前锁定的 `tauri-plugin-updater 2.13.1` 支持 AppImage/DEB/RPM 和 Mac 应用归档。自定义 `install(bytes)` 不负责验签，因此现有竞速下载在每个候选胜出前校验 minisign 签名与受签名保护的版本，GitHub 提供 SHA-256 时同时校验摘要。缺失签名、版本不符、错误的包名或格式均停止安装，不降级为无校验更新。

Release API 内的清单资产地址优先于备用地址。Gitee 备用清单地址为 `https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager/releases/download/latest/latest.json`，`download` 位于 `latest` 前；GitHub 同名仓库仍使用 `releases/latest/download/latest.json`。后端更新 API、手动更新页面和备用清单地址与用户配置的更新仓库保持一致。清单版本及包名必须与 API 选出的版本和本平台包一致，不能靠另一版本的清单或单独 `.sig` 绕过此绑定。

## 发布前置条件

`.github/workflows/build.yml` 使用 Windows、Ubuntu 22.04、Mac Apple Silicon 和 `macos-15-intel` 四个 runner。正式构建上传到当前代码仓库 `CH3NGYZ/Overcooked-2-MOD-Manager` 的同一草稿（原名 `Overcooked-2-MOD-Manager-Code`，仓库与历史保持不变）；在四个构建成功后，由独立任务验证实际上传字节和签名，再汇总七目标 `latest.json`。并行构建不各自上传清单，也不自动发布草稿。新建草稿绑定实际构建的 `GITHUB_SHA`，不指向另一个仓库或无关提交。

发布任务直接使用 GitHub 自动创建的 `secrets.GITHUB_TOKEN`，只有准备草稿、上传包和汇总清单三个任务声明 `contents: write`。默认权限保持只读，不再依赖跨仓库 `RELEASE_REPOSITORY_TOKEN`，也不把本机 API Token 复制到 Actions。代码仓库仍须配置以下 updater Secrets：

- `TAURI_SIGNING_PRIVATE_KEY`：与 `src-tauri/tauri.conf.json` 的 updater 公钥对应的私钥文件完整内容，不是本机文件路径。
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：私钥有密码时配置。

这些是 updater 的 minisign 密钥，不是 Apple 开发者证书或 Windows 代码签名证书。操作系统代码签名和 macOS 公证需单独配置与验收。用户已更换当前源码公钥，Actions 测试应显式使用 `signing=configured` 验证其配对私钥，不得换成临时公钥冒充正式密钥验收。

用户自行创建长期密钥时可运行 `npm run tauri -- signer generate -w .tauri/updater.key`；生成器会显示私钥，输出不得粘贴到聊天或日志。`.key.pub` 文件内容是一整行 Base64，解码后为 minisign 公钥文本（含注释及公钥行），不是 PEM 证书、摘要或私钥。将 `.key.pub` 的完整内容填写到客户端 `plugins.updater.pubkey`，将 `.key` 的完整内容填写到 Actions 的 `TAURI_SIGNING_PRIVATE_KEY` Secret，有密码时另填密码 Secret；`.tauri/` 已忽略，仍应离线备份私钥。本轮使用用户已有的新密钥，不再生成生产密钥。

旧 Windows 客户端原本直接运行 EXE/MSI，未用 Updater 公钥验证，可按旧入口安装内置新公钥的新版本，然后进入统一签名更新链；需保留旧包名并发布更高应用版本。已使用旧公钥验签的客户端不能自动接受新私钥签名，除非有旧私钥签署过渡包，否则需手动安装一次。当前 `1.2.0` 测试不等于发布了可被已有 `1.2.0` 自动识别的更高版本。

正式草稿工作流由 `v<项目版本>` 标签或手动 dispatch 的 `mode=release` 触发；项目的 package、Tauri、Cargo 及两个 lock 文件版本必须一致。发布 dispatch 使用项目版本生成标签。已公开的同名 Release 不允许覆盖。`scripts/build-release.mjs` 对包括 DEB/RPM 在内的更新包补签并检查公钥；聚合任务检查缺包、重复包、错误下载 URL、摘要、签名与版本后才上传清单。正式推送重建标签时以该次标签 push 触发，不额外 dispatch 同一提交造成重复构建。

GitHub 草稿的资产 API 可能返回 `releases/download/untagged-<临时标识>/...`。聚合器只接受正式版本路径或同一代码仓库、同一草稿 `html_url` 对应的临时路径，实际字节仍从带认证的资产 ID 接口下载并完整验签。`latest.json` 始终写 `releases/download/v<版本>/...`，不把公开后会变化的草稿路径留在更新清单中。同名草稿重试会更新 `target_commitish` 元数据，PATCH 必须同时显式保留 `tag_name`、`draft: true` 和 `prerelease: false`；实际 API 已验证仅提交 `target_commitish` 会把草稿标签重置为 `untagged-...`，导致后续身份保护拒绝修改。修改前后均校验同版本稳定草稿，不能放宽已公开版本保护。GitHub 的 `target_commitish` 不会移动已有标签；实际源码以明确授权的 Git 标签引用及 Actions SHA 为准，重试优先对同一标签使用限定旧对象的 lease 更新，避免删除期间草稿脱离关联。

客户端的 GitHub/Gitee 更新渠道按用户配置指向 `Overcooked-2-MOD-Manager`，GitHub 与更名后的当前代码仓库一致。本仓库生成的清单 URL 指向本仓库实际资产；Gitee 或后续其他仓库的同步流程仍由用户另行实现，需同步完整更新包及各自 `.sig` 原始字节，并将 `latest.json` 的 URL 改为目标仓库对应的实际资产地址，不重压缩或改签名。仅生成草稿不代表客户端的公开更新渠道已有可用更新。本轮不实现跨仓库或 Gitee 同步。

根目录 `index.html` 是已忽略的生成文件，不再纳入 Git 或提交历史。`npm run dev` 和 `npm run build` 的生命周期钩子先调用 `scripts/frontend-entry.mjs` 生成原有 Vite 入口，因此 Tauri 和 Actions 的干净 checkout 仍能构建；入口标题、favicon、语言及应用挂载点保持原样。不要直接依赖未生成入口时的裸 `vite` 命令。

## 本地 BAT 与 Actions 的 NSIS 一致性

`build.bat`、`build with tag.bat` 和 `build.log` 仅保留在本地并明确加入 `.gitignore`，不分发到 GitHub 当前源码。没有这两个 BAT 的干净 checkout 只跳过对应本地入口回归，共享签名、版本一致性、资产选择和发布保护回归继续执行。自动打包直接使用已跟踪的 `scripts/build-release.mjs`，不依赖本地 BAT。

`build.bat` 使用同一个 `scripts/build-release.mjs`，显式指定 `x86_64-pc-windows-msvc` 与 `--bundles nsis`。它读取相同 Tauri 配置与 NSIS 模板，生成应用版本绑定的 updater 签名，并以当前公钥验签后才进入原有安装、启动流程。Actions 默认仍生成该平台的全部包；本地 NSIS-only 不要求额外生成 MSI，也不跳过签名或更换公钥。

`build.bat` 的命令和注释保持 ASCII、无 BOM、CRLF，避免 CMD 在 `chcp 65001` 后将 UTF-8 中文注释切成命令片段。构建入口经过 BAT、PowerShell 与内层 CMD，`2>&1` 只由内层 CMD 解释，不能转义成传给 Node 的字面参数；否则会被共享脚本当成额外 bundle 拒绝。Windows PowerShell 5 的 `Tee-Object` 默认写 UTF-16，因此构建日志用显式 UTF-8 流追加，与 BAT 的日志编码保持一致；stdout/stderr 同时显示并保存，构建退出码独立传回 BAT。

`.gitattributes` 对 BAT 使用 `text eol=crlf`：Git 中仍保存规范化 LF，Windows、Linux 和 macOS 检出均为 CRLF，不依赖用户的 `core.autocrlf`。回归通过真实 `git checkout-index` 并禁用自动平台转换，确认两个 BAT 均为 CRLF 且无 BOM；不跳过 Unix runner 的编码检查，也不执行安装器。

本地 updater 私钥使用 `TAURI_SIGNING_PRIVATE_KEY` 的完整内容，或 `TAURI_SIGNING_PRIVATE_KEY_PATH` 指向离线备份的私钥文件；有密码时配置 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。BAT 从当前进程环境读取，并用当前用户的注册表环境作缺失值后备，只记录是否配置。GitHub Secret 不会自动下载到本机。`noenv` / `clean` 保留原有行为，只清空两个 `OC2_INSTALLER_*_TOKEN`，仍要求有效 updater 签名；缺少签名密钥则停止，不生成能安装但无法验证的替代包。

产物位于 `<CARGO_TARGET_DIR>/x86_64-pc-windows-msvc/release/bundle/nsis/`，未指定 target dir 时为 `src-tauri/target`。安装阶段只选择当前项目版本的 `OC2ModManager_<版本>_x64-setup.exe`，并要求对应 `.sig` 存在，不再按时间选择旧文件。这里的一致性是打包配置、格式与签名校验规则一致，不承诺不同机器/工具链编译出的 EXE 字节完全相同。

`build with tag.bat` 在调用共同入口前事务性更新 Cargo.toml、Cargo.lock 的 `app`、Tauri、package.json 及 package-lock.json 的两个项目版本。依赖版本保持原样；格式或写入失败回滚，`noenv` / `clean` 参数继续原样转发。脚本回归在已忽略的工作区副本中运行嵌入 Node 更新器，Windows 额外运行真实版本 BAT 加无安装行为的 build stub；不在回归中执行生产安装器或启动应用。

`node --test tests/local-build.test.mjs` 还覆盖真实构建 BAT 的隔离副本：936/65001 初始代码页、default/noenv/clean、空格和中文路径、真实 `buildUpdateKeys` 参数校验、中文 stdout/stderr 与 UTF-8 日志，以及失败退出和成功后缺少当前版本安装包时的拒绝安装。回归在共享签名编译入口前替换为无安装探针，不执行编译器、生产签名、安装器或管理器，不能替代完整打包验收。

## 非发布四端 Actions 打包测试

手动运行已有 `build.yml`，选择测试分支、`mode=test` 和 `signing=configured`（默认值），运行同一四端矩阵，但跳过所有 Release 准备、上传和清单发布步骤。测试不写入 Release；配置密钥模式使用代码仓库的私钥及密码 Secrets，打包前用真实 Tauri CLI 签名一个临时载荷并以当前源码公钥验签。缺少密钥、错误密码或公私钥不匹配均提前失败，不自动降级为临时签名。

`scripts/ci-packages.mjs` 的配置密钥模式保留源码公钥，私钥仅经 Actions 步骤环境传入签名子进程，不写入 `GITHUB_ENV` 或产物，预检输出全部捕获。无生产 Secret 时仍可显式选择 `signing=temporary`：只在 runner 临时检出生成测试密钥、替换公钥，捕获生成器全部输出并在任务结束删除临时私钥。两种模式都禁用测试包的配置更新地址，并复用 `build-release.mjs` 打包验签。

临时私钥路径使用 `TAURI_SIGNING_PRIVATE_KEY_PATH`；共用 CLI 边界仅为 `tauri build` 映射为其接受的 `TAURI_SIGNING_PRIVATE_KEY` 路径，`signer sign` 仍使用 `TAURI_SIGNING_PRIVATE_KEY_PATH`。配置密钥则使用内联完整内容，不通过路径变量传递。聚合任务要求四个报告的模式一致，配置密钥模式还要求公钥与检出源码完全相同，逐个复核全部实际包字节和版本签名后，用正式流程共用的清单生成器生成七目标 `latest.json`。

每个平台上传 `package-test-<Rust target>`，包含包、签名以及标明 `testOnly`、签名模式的版本/提交/摘要报告。两个 Mac DMG 加上七种更新格式共九个包；独立聚合任务重新下载并核对四个构建目标、同一提交、字节摘要及七种签名，结果保存为 `four-platform-package-summary`；配置密钥模式同时保存 `latest.json`，清单的 Release URL 仅用于结构验证，尚未公开上传。构建日志另存 `package-build-log-<Rust target>`。这些 Actions 产物保留七天，不创建或修改 Release，不执行安装器或游戏。

临时测试签名不证明生产密钥匹配；配置密钥测试证明当前 Actions 私钥与客户端公钥及实际包签名匹配。两种测试都不证明 Apple 签名、公证、系统权限或实际自动更新成功，测试包与测试清单均不能当作已发布更新。

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

2026-10-13 的统一 Updater 与配置密钥四端测试已完成：代码提交 `e0068958910c513f9dd051f057c1856fe12dc6d0`，Actions 运行 [38326592135](https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Code/actions/runs/38326592135) 的 Windows、Linux、Mac ARM64、Mac Intel 和聚合任务均成功。测试显式使用 `mode=test, signing=configured`；四端预检均用 Actions 私钥签名，并通过当前源码公钥校验，没有生成或替换生产公钥。每端 Node 22 项；Rust Windows 为 278 通过，另外三端为 272 通过，各有 2 项显式忽略，原 Windows 更新入口的两条 dead_code 警告未再出现。

CI 聚合重新下载并校验九个包、七份 updater 签名及 SHA-256，包含两个 Mac DMG 和双架构 `.app.tar.gz`，并生成包含七个目标键的 `latest.json`。已下载 [four-platform-package-summary](https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Code/actions/runs/38326592135/artifacts/11782050439)，本地核对 `testOnly=true`、`signingMode=configured`、版本 `1.2.0`、代码提交、全部包记录、公钥与源码一致以及清单与报告相互对应。实际包字节和签名由 CI 验证，本地只下载摘要及清单核对，不把摘要检查说成再次下载了全部安装包。

本轮证据位于 `src-tauri/target/verification/unified-updater-20261013`，包括四端/聚合日志、运行快照、摘要、`run-38326592135-latest.json` 及 `run-38326592135-verification.json`。测试未创建或修改 Release、main 或标签；正式清单上传任务按测试模式跳过，测试清单 URL 尚未公开发布，产物保留七天。该结果证明当前 Actions 私钥、源码公钥及实际更新包签名匹配，不证明实际安装更新或系统代码签名。

此前临时密钥运行 [38319473951](https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Code/actions/runs/38319473951) 仅覆盖 `59be3ed` 的旧更新入口；该阶段 Windows 本机三种官方 BepInEx 原包文件系统回归仍可追溯，但不作为本轮 Windows Updater 或新公钥验收。旧 CI 心跳保持暂停，没有重启监控；以下真实安装、更新及游戏验收仍未进行。

正式 `1.2.0` 草稿运行 [38334863349](https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Code/actions/runs/38334863349) 已全部成功：源码及实际 `v1.2.0` 标签均绑定 `4ef16c26a7712cab29902efc3ca50be5901d8e5b`，Prepare、Windows、Linux、双架构 Mac 和清单聚合均通过。Windows Node 为 33 通过，另外三端各 32 通过、1 项 Windows 专属 BAT 测试跳过；Rust Windows 278 通过，另外三端各 272 通过，均 0 失败、2 显式忽略。聚合重新下载七种更新包实际字节，完成长度、摘要、当前公钥签名和版本校验后上传七键 `latest.json`，所有 URL 为正式 `v1.2.0` 路径。最终本仓库草稿 `411128097` 含九包、七份签名和清单，合计恰好 17 项，仍 `draft=true`、`published_at=null`；本地只读核验额外核对实际 Git 标签、元数据、清单和七份签名文件，不代替 CI 的完整包字节验签。证据位于 `src-tauri/target/verification/release-120-20261013`。本轮没有公开草稿、同步其他仓库、更换密钥或执行实际安装更新。

### 1.2.1 草稿：账单阻塞已解除，四端完成

社区仓库简介、官方去重、全部 ZIP/DLL 安装入口及 Gitee README 单次直连修复已提交并升为 `1.2.1`。本地最终 Node 为 52 通过，Rust 为 303 通过、0 失败、2 项显式忽略，fmt/check 和前端构建通过；实际 Vue 页面与 IPC 夹具的 30+4 项检查通过。这不代表真实 MOD 安装或游戏验收。

第二轮 [38354780108](https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Code/actions/runs/38354780108) 的 Windows 和 Linux 打包成功，五种更新格式通过当前公钥与版本绑定验签。两个 Mac 均在 README 大小限制测试的同步服务器线程收尾卡住，达到 60 分钟工作流上限后取消；统一清单因此跳过。收尾已改为 `spawn_blocking` 并限定测试服务器写超时，有界调度回归先红后绿；生产 README 的大小、404、取消及 Gitee 下载规则没有变化。

当时 `v1.2.1` 标签指向 `dbb624fc6bc91f3953160490f392da72fca3cd0f`。运行 [38364610804](https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Code/actions/runs/38364610804) 绑定该源码，但 Prepare 未执行任何步骤；GitHub annotation 提示近期付款失败或消费限额需要提高，并要求检查 `Billing & plans`，不是代码或私钥失败。这是历史阻塞，不代表当前仍受限。

旧草稿 `411329219` 当时只有五种 Windows/Linux 包及各自签名，共 10 项，没有 Mac 包和 `latest.json`；相关历史证据位于 `src-tauri/target/verification/community-121-20261013`。

账号恢复、Agent 文件历史清理及 1.2.1 标签重建后，最终运行 [38385940925](https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager/actions/runs/38385940925) 的 Prepare、Windows、Linux、双架构 Mac 和统一清单均成功。该次 CI 源码为 `38516614b87487c287d6a91bd51bc8d72e1ea25d`；草稿 `411541254` 恰有九包、七签名和七键 `latest.json` 共 17 项，仍未公开。2026-10-14 已只读复核运行 success 与草稿状态；完整历史证据位于 `src-tauri/target/verification/release-121-history-clean-20261013`。后续 index.html 历史重写会改变 Git SHA，不改变这些历史运行的实际源码记录或已生成包字节。

### 仍待实机验收

1. 在四个构建环境分别确认格式、编译、完整 Rust 回归及签名产物；检查草稿清单包含七个键，未签名和坏签名包均无法安装。
2. 每种安装格式从较旧版本更新，检查只选择本格式/架构、权限失败可重试、取消保留旧程序、安装失败有提示，以及成功后版本与重启结果。
3. 确认用户目录产生配置、日志、清单和备份；切换安装位置或整包更新不覆盖新数据根；旧安装目录数据不被读取或删除。
4. Linux/macOS 分别验证默认 Wine、切换原生、重启后模式持久化、对应 Steam 探测、外置库、游戏运行检测和框架开关；双模式同时存在时不能串用路径或 prefix。
5. 原生安装后通过 Steam 启动并检查 `BepInEx/config/BepInEx.cfg`、`BepInEx/LogOutput.log`；Wine 验证目标容器 winhttp 覆盖、已有 Steam 重启、MOD UI 字体及 Mac CrossOver 向导。

已完成的本地与配置密钥四端 CI 验证，不替代正式 Release/Gitee 资产同步、各平台实机安装更新、系统代码签名及公证、Steam/Wine 运行和实际游戏加载验收。
