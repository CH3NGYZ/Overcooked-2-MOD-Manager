# OC2 Mod Manager

基于 Rust、Tauri 2、Vue 3 和 TypeScript 的《胡闹厨房 2》Windows x64 MOD 管理器。

这个项目支持从 GitHub、Gitee Release 获取 MOD 版本和资产，也支持通过一个公开的 TOML 文件制作和分享自己的仓库源。

## 仓库源和导出配置的区别

仓库源是一个供程序在线读取的 TOML 文件，可以包含一个或多个 `[[repositories]]` 条目，也可以通过 `[[repository_sources]]` 条目继续声明其他仓库源。程序读取仓库源后，会保存并刷新其中的仓库源，再读取这些子仓库源中的仓库。

```toml
[[repositories]]
name = "我的 MOD"
provider = "github"
url = "https://github.com/example/my-mod/releases"
channel = "stable"
install_target = "BepInEx/plugins"
plugin_guid = "com.example.my-mod"
local_product_name = "My Mod"
local_original_filename = "MyMod.dll"
download_threads = 16
api_proxy_mode = "auto"
api_proxy_url = ""
release_proxy_mode = "auto"
release_proxy_url = ""
enabled = true

[[repositories.asset_rules]]
type = "exact"
value = "MyMod.dll"
```

仓库源不需要，也不应该添加下面这些导出配置包字段：

- `format`
- `schema_version`
- `exported_at`

这些字段只属于软件“导出配置”生成的完整配置包。完整配置包用于在另一台管理器中导入，不能直接替代仓库源文件。

仓库源的推荐扩展名是 `.oc2repo.toml`，例如 `my-repositories.oc2repo.toml`。文件必须使用 UTF-8 编码，并且能够通过公开的 HTTP 或 HTTPS 地址访问。

### 在仓库源中声明其他仓库源

仓库源文件可以使用 `[[repository_sources]]` 声明子仓库源。子仓库源会在父仓库源刷新成功后自动加入仓库管理，并继续递归刷新：

```toml
[[repository_sources]]
id = "azhe-Others"
nickname = "Azhe的其他MOD仓库源"
url = "https://raw.giteeusercontent.com/ch3ngyz/Overcooked-2-MOD-Manager-Releases/raw/main/azhe-Others.oc2repo.toml"
region = "international"
builtin = true
```

`id` 就是管理器中显示和填写的“仓库唯一 ID”，在当前仓库源文件中必须唯一；`url` 必须是公开的 HTTP 或 HTTPS 地址。`builtin = true` 表示该源由仓库源作者提供，管理器会显示为内置源，不能在“管理仓库源”窗口中编辑、删除或拖动。`region` 是可选字段：`chinese_mainland` 只对中国大陆显示，`international` 只对其他地区显示；省略时对所有地区显示。嵌套源不需要填写 `parent_id`，管理器会根据声明它的父源自动维护父子关系。

仓库源文件可以同时包含仓库和子仓库源。刷新父源时，管理器会先更新父源声明的仓库源，再刷新子仓库源和各自的 Release；如果父源移除了某个子仓库源，管理器也会移除该子源及其仓库绑定。管理器会防止同一地址重复导入，并避免循环引用造成无限刷新。

## 如何制作仓库源

### 1. 找到 Release 地址

目前每个仓库都应该对应一个 GitHub 或 Gitee Release 页面，地址通常是：

```text
https://github.com/所有者/仓库名/releases
https://gitee.com/所有者/仓库名/releases
```

`provider` 必须和地址匹配：GitHub 地址使用 `github`，Gitee 地址使用 `gitee`。

### 2. 查看要安装的资产文件名

打开 Release，确认真正需要下载的文件名。例如：

- 单个插件：`MyMod.dll`
- 压缩包：`MyMod.zip`
- 带版本号的压缩包：`MyMod_v1.2.3.zip`

然后用资产规则匹配它。规则可以有多个，多个规则之间是“满足任意一个即可”。

```toml
[[repositories.asset_rules]]
type = "exact"
value = "MyMod.dll"

[[repositories.asset_rules]]
type = "exact"
value = "MyMod.zip"
```

### 3. 选择资产规则类型

#### 精确匹配

适合文件名固定的资产，匹配时不区分大小写：

```toml
[[repositories.asset_rules]]
type = "exact"
value = "HostUtilities.zip"
```

#### 通配符匹配

支持 `*` 和 `?`，也不区分大小写：

```toml
[[repositories.asset_rules]]
type = "glob"
value = "HostUtilities_*.zip"
```

#### 正则表达式匹配

使用 Rust 正则表达式语法。需要匹配整个文件名时，建议加上 `^` 和 `$`：

```toml
[[repositories.asset_rules]]
type = "regex"
value = '^BepInEx\.ConfigurationManager_BepInEx5_v\d+\.\d+\.zip$'
```

单引号 TOML 字符串适合保存正则表达式，因为反斜杠不需要再次转义。正则表达式必须能够被 Rust `regex` 库解析，不能使用回溯、反向引用等不支持的语法。

### 4. 设置安装目录

`install_target` 是相对于游戏目录的安全相对路径：

```toml
install_target = "BepInEx/plugins"
```

如果压缩包本身已经包含 `BepInEx` 顶层目录，可以设置为空字符串，让管理器按照压缩包内部结构安装：

```toml
install_target = ""
```

不要填写绝对路径，也不要使用 `..` 跳出游戏目录。

### 5. 填写本地 MOD 识别信息

管理器会用 GUID、DLL 的 `ProductName` 和 `OriginalFilename` 将本地 MOD 绑定到仓库。建议尽可能填写准确：

```toml
plugin_guid = "com.example.my-mod"
local_product_name = "My Mod"
local_original_filename = "MyMod.dll"
```

如果插件没有 BepInEx GUID，至少填写 DLL 的产品名称或原始文件名。`plugin_guid` 不能用来替代资产规则；它负责识别本地插件，`asset_rules` 负责选择 Release 下载文件。

## 仓库源完整示例

下面的示例展示了分组、多语言名称、多语言简介、Markdown 简介和正则资产规则：

```toml
[[repositories]]
name = "BepInEx 配置管理器"
name_i18n = { "zh-CN" = "BepInEx 配置管理器", "zh-TW" = "BepInEx 設定管理器", "en" = "BepInEx Configuration Manager", "ko" = "BepInEx 구성 관리자" }
group = "前置必装"
group_i18n = { "zh-CN" = "前置必装", "zh-TW" = "前置必裝", "en" = "Required prerequisites", "ko" = "필수 선행 항목" }
provider = "github"
url = "https://github.com/BepInEx/BepInEx.ConfigurationManager/releases"
channel = "all"
install_target = ""
plugin_guid = "com.bepis.bepinex.configurationmanager"
local_product_name = "BepInEx.ConfigurationManager"
local_original_filename = "ConfigurationManager.dll"
download_threads = 16
api_proxy_mode = "auto"
api_proxy_url = "https://oc2ghapi.ch3ng.top"
release_proxy_mode = "auto"
release_proxy_url = "https://oc2gh.ch3ng.top"
enabled = true

[repositories.note_i18n]
"zh-CN" = """
## 使用说明

- 这是创建配置文件的 MOD 依赖。
- 默认快捷键是 `F1`，也可能被其他 MOD 改为 `P`。
"""
"zh-TW" = "這是建立設定檔的 MOD 依賴，預設快捷鍵是 `F1`。"
"en" = "This is a dependency for mods that create configuration files. The default hotkey is `F1`."
"ko" = "설정 파일을 만드는 MOD의 의존성입니다. 기본 단축키는 `F1`입니다."

[[repositories.asset_rules]]
type = "regex"
value = '^BepInEx\\.ConfigurationManager_BepInEx5_v\\d+\\.\\d+\\.zip$'

[[repositories]]
name = "主机实用 MOD 稳定版 - Gitee"
name_i18n = { "zh-CN" = "主机实用 MOD 稳定版 - Gitee", "zh-TW" = "主機實用 MOD 穩定版 - Gitee", "en" = "HostUtilities Stable - Gitee", "ko" = "HostUtilities 안정 버전 - Gitee" }
group = "主机实用 MOD 稳定版"
group_i18n = { "zh-CN" = "主机实用 MOD 稳定版", "zh-TW" = "主機實用 MOD 穩定版", "en" = "HostUtilities Stable", "ko" = "HostUtilities 안정 버전" }
provider = "gitee"
url = "https://gitee.com/ch3ngyz/Overcooked-2-HostUtilities-Stable-Releases/releases"
channel = "all"
install_target = "BepInEx/plugins"
plugin_guid = "com.ch3ngyz.plugin.HostUtilities"
local_product_name = "Overcooked! 2 HostUtilities"
local_original_filename = "HostUtilities.dll"
download_threads = 1
api_proxy_mode = "auto"
api_proxy_url = "https://oc2gtapi.ch3ng.top"
release_proxy_mode = "direct"
release_proxy_url = ""
enabled = true

[repositories.note_i18n]
"zh-CN" = """
稳定版街机 MOD，提供主机控制、延迟查看、跳关和重开等功能。
"""
"en" = "Stable arcade MOD with host controls, latency display, level skipping and restart tools."

[[repositories.asset_rules]]
type = "exact"
value = "HostUtilities.zip"
```

项目中的 [builtin-repositories.toml](src-tauri/resources/builtin-repositories.toml) 也可以作为更完整的参考。远程仓库源可以使用相同的 `repositories` 条目，但不需要复制 `id`；仓库 ID 会由管理器在导入时生成和维护。内置仓库配置中的 `id` 同样是可选的，因此可以直接粘贴软件导出的 `[[repositories]]` 条目；省略时管理器会根据 Provider、Release 地址、资产规则和安装目录生成稳定 ID，并忽略导出内容中的 `source_id`。

## 多语言名称、分组和简介

### 名称

`name` 是默认名称，`name_i18n` 是可选的多语言名称表：

```toml
name = "主机实用 MOD"
name_i18n = { "zh-CN" = "主机实用 MOD", "zh-TW" = "主機實用 MOD", "en" = "HostUtilities", "ko" = "HostUtilities" }
```

支持的语言键是：

- `zh-CN`：简体中文
- `zh-TW`：繁體中文
- `en`：English
- `ko`：한국어

当前语言没有对应翻译时，管理器会回退到 `name`。

### 分组

相同的 `group` 会显示在同一个可展开、可收起的分组卡片中：

```toml
group = "主机实用 MOD 稳定版"
group_i18n = { "zh-CN" = "主机实用 MOD 稳定版", "zh-TW" = "主機實用 MOD 穩定版", "en" = "HostUtilities Stable", "ko" = "HostUtilities 안정 버전" }
```

分组名称由 TOML 决定，管理器不会根据 GUID 或仓库名称自动猜测分组。没有填写 `group` 的仓库会放入未分类仓库。

### Markdown 简介

`note` 和 `note_i18n` 支持 Markdown。简介较短时会直接显示在仓库卡片中，内容较长时会在二级菜单中渲染。多行简介使用 TOML 三引号：

```toml
note = """
## 功能

- 提供主机控制
- 显示延迟
- 支持跳关
"""
```

多语言简介可以使用 `[repositories.note_i18n]` 表，每种语言单独填写：

```toml
[repositories.note_i18n]
"zh-CN" = """
## 功能

- 提供主机控制
- 显示延迟
"""
"en" = "Provides host controls and latency display."
```

不要把访问令牌、密码、Cookie 或其他秘密信息写入 `note`、URL 或仓库源文件。

## 可用字段参考

| 字段 | 是否必填 | 说明 |
| --- | --- | --- |
| `name` | 是 | 默认显示名称。 |
| `name_i18n` | 否 | 多语言名称表。 |
| `group` | 否 | 分组的稳定键；为空时进入未分类仓库。 |
| `group_i18n` | 否 | 分组的多语言显示名称。 |
| `provider` | 是 | `github` 或 `gitee`。 |
| `url` | 是 | GitHub/Gitee Release 页面地址。 |
| `channel` | 是 | `stable`、`beta` 或 `all`。稳定版排除 prerelease，测试版只取 prerelease，全部版本包含两者；草稿 Release 不会使用。 |
| `asset_rules` | 是 | 一个或多个资产规则，至少一条。多个规则满足任意一条即可。 |
| `install_target` | 是 | 游戏目录下的相对安装目录；空字符串表示按压缩包内部结构安装。 |
| `note` | 否 | 默认 Markdown 简介。 |
| `note_i18n` | 否 | 多语言 Markdown 简介表。 |
| `plugin_guid` | 是 | 本地 BepInPlugin GUID，用于绑定已安装 MOD。 |
| `local_product_name` | 否 | DLL 的 ProductName，用于本地绑定。 |
| `local_original_filename` | 否 | DLL 的 OriginalFilename，用于本地绑定。 |
| `download_threads` | 否 | GitHub 下载线程范围为 1 到 32，默认 16；Gitee 强制单线程。 |
| `api_proxy_mode` | 否 | `direct`、`auto` 或 `fixed`，控制 Release API 请求。默认 `auto`。 |
| `api_proxy_url` | 否 | API 代理地址；没有代理时留空。 |
| `release_proxy_mode` | 否 | `direct`、`auto` 或 `fixed`，控制资产下载。默认 `auto`。 |
| `release_proxy_url` | 否 | Release 下载代理地址；没有代理时留空。 |
| `enabled` | 是 | `true` 显示并刷新，`false` 暂停该仓库。 |

仓库源中的每个 `[[repositories]]` 都可以直接来自软件的导出配置。导出内容里的 `source_id` 只代表导出时的本地来源，发布仓库源时不需要手动修改；管理器下载仓库源后会忽略这些旧值，并统一写入当前仓库源配置的唯一 ID。

## 代理配置建议

API 代理和 Release 下载代理是两套独立配置：

```toml
api_proxy_mode = "auto"
api_proxy_url = "https://example.com/github-api-proxy"
release_proxy_mode = "auto"
release_proxy_url = "https://example.com/github-release-proxy"
```

- `direct`：只请求官方地址。
- `auto`：先请求官方地址，失败后再尝试代理地址。
- `fixed`：直接使用代理地址。

代理地址可以包含 `{url}` 占位符。没有占位符时，管理器会按照代理服务的路径规则拼接目标 URL。制作仓库源时应优先使用稳定、公开、无需登录的代理地址；不要把 Token 写进代理 URL。

Gitee 通常使用单线程下载，GitHub 可以使用多线程下载。仓库源中的线程数只是下载设置，不能改变 Release API 的返回内容。

## 在管理器中添加仓库源

1. 将 `.oc2repo.toml` 上传到一个稳定的公开地址。
2. 用浏览器或 `curl` 确认地址返回的是 TOML 原文，而不是 HTML 页面、登录页或 JSON 错误。
3. 打开 OC2 Mod Manager 的“仓库管理”。
4. 打开“管理仓库源”，点击“添加仓库源”。
5. 填写仓库源昵称和 TOML 文件地址。
   新建仓库源时还要填写“仓库唯一 ID”。它只能包含字母、数字、点、短横线和下划线，且首字符必须是字母或数字，例如 `my-source-id`。
6. 点击“保存仓库源”。只有保存后，管理器才会请求仓库源文件。
7. 文件解析成功后，文件中的仓库会出现在对应的仓库源和分组中；文件中的 `[[repository_sources]]` 也会被加入并自动刷新其下级仓库源和 Release。

从软件导出仓库配置并粘贴到仓库源文件时，可以保留导出的 `source_id`，管理器会在下载时自动忽略它，并把所有仓库绑定到“仓库唯一 ID”对应的仓库源。这样同一份导出内容可以复制到不同仓库源，而不会继承旧的来源 ID。

如果自定义仓库源下载或解析失败，管理器会显示失败提示并保留该源之前的仓库。根内置源的回退规则见下方“内置仓库源配置”。

### 内置仓库源配置

程序随安装包读取 `src-tauri/resources/builtin-repositories.toml` 中的 `[[repository_sources]]`。这个文件是内置仓库和内置仓库源的唯一配置入口，适合在发布前集中修改昵称、地址和其他内置参数。当前内置源为：

```toml
[[repository_sources]]
id = "azhe-HostUtilities"
nickname = "Azhe的仓库源 - Gitee"
url = "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Releases/raw/main/all-repository-source.oc2repo.toml"
region = "chinese_mainland"
builtin = true

[[repository_sources]]
id = "azhe-Others"
nickname = "Azhe的仓库源 - Github"
url = "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Releases/raw/refs/heads/main/all-repository-source.oc2repo.toml"
region = "international"
builtin = true
```

每次启动完成程序更新检查后，管理器都会重新读取这两个内置源的定义。管理器根据 Windows 当前用户的国家/地区设置选择当前地区，只请求匹配的仓库源；不匹配的源仍保存在配置中，但不会出现在仓库列表，也不会参与自动刷新，仍可在“管理仓库源”窗口中查看和修改。内置源及其远程声明的 `builtin = true` 子源不能编辑、删除或拖动；用户添加的源仍可管理。启动时会请求匹配的内置源，成功后按源文件解析仓库和子仓库源，并重新绑定本地 MOD。根文件请求或解析失败时，该内置源使用本地内置仓库回退；如果根文件成功但只声明了子源，则只有根文件没有 `repositories` 且所有子源都失败时才触发回退。根文件自身包含仓库时，即使全部子源失败也不会回退。回退时仓库管理会隐藏失败的远程内置源并显示“内置仓库源”分组，其中包含安装包自带的本地内置仓库。

## 发布前检查清单

- [ ] 文件是 UTF-8 编码的 TOML。
- [ ] 顶层至少存在一个 `[[repositories]]` 或 `[[repository_sources]]`。
- [ ] 没有加入 `format`、`schema_version`、`exported_at` 等导出包字段；`[[repository_sources]]` 是仓库源文件允许使用的条目。
- [ ] 每个仓库的 `provider` 与 URL 域名一致。
- [ ] 每个仓库至少有一条有效的 `asset_rules`。
- [ ] 资产规则能匹配 Release 中真实存在的文件名。
- [ ] `plugin_guid`、`local_product_name`、`local_original_filename` 与 DLL 实际信息一致。
- [ ] `install_target` 是相对路径，没有绝对路径和 `..`。
- [ ] 多语言名称、分组和简介使用固定语言键。
- [ ] 需要按地区显示时，为仓库源填写 `region = "chinese_mainland"` 或 `region = "international"`；省略表示所有地区。
- [ ] 简介中的 Markdown 使用三引号保存多行内容。
- [ ] URL 不包含 Token、密码或 Cookie。
- [ ] 用浏览器确认远程文件可以直接返回原始 TOML。

## 开发环境

需要 Rust MSVC toolchain、Windows SDK、Node.js 18+ 和 WebView2 Runtime。项目使用 `x86_64-pc-windows-msvc` 目标。

```powershell
npm install
npm run build
```

开发模式：

```powershell
npx tauri dev
```

正式构建按项目根目录的 `build.bat` 执行。配置和缓存目录位于当前 EXE 同级目录：

```text
config.toml
cache\\
manifests\\
backups\\
```

Token 使用 Windows DPAPI 保护，不会写入仓库源、导出文件、日志或代理请求 URL。
