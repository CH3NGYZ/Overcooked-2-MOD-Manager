# OC2 MOD Manager 仓库源配置指南

本文面向《胡闹厨房 2》MOD 作者和仓库源维护者，说明如何制作、分享和维护管理器能够读取的 TOML 仓库源。

一个 MOD 仓库条目通过 `provider`、`username`、`repository` 指定 GitHub 或 Gitee 项目，可选择下载 Release 资产或指定分支中的单个文件。仓库源文件可以包含多个 MOD 仓库，也可以引用其他仓库源，组织成父源和子源。具体 MOD 仓库不填写 `url`；仓库源和代理来源继续使用 `url` 指向文件。

## 仓库源和导出配置的区别

仓库源是一个供程序在线读取的 TOML 文件，可以包含一个或多个 `[[repositories]]` 条目，也可以通过 `[[repository_sources]]` 条目继续声明其他仓库源。程序读取仓库源后，会保存并刷新其中的仓库源，再读取这些子仓库源中的仓库。

```toml
[[repositories]]
name = "我的 MOD"
provider = "github"
username = "example"
repository = "my-mod"
download_mode = "release"
region = "international"
channel = "all"
install_target = "BepInEx/plugins"
plugin_guid = "com.example.my-mod"
local_product_name = "My MOD"
local_original_filename = "MyMod.dll"
version_regex = ''
max_release_count = 10
download_threads = 16
enabled = true

[[repositories.asset_rules]]
type = "exact"
value = "MyMod.dll"
```

若 MOD 文件直接保存在分支中，使用 `repository_file` 模式。管理器根据平台、用户名、仓库名、分支和相对路径生成 Raw 下载地址：

```toml
[[repositories]]
name = "OC2 Many Recipes"
provider = "github"
username = "gua248"
repository = "Overcooked2-ManyRecipes"
download_mode = "repository_file"
branch = "master"
download_threads = 4
file_path = "bin/Release/OC2ManyRecipes.dll"
plugin_guid = "dev.gua.overcooked.manyrecipes"
local_product_name = "OC2ManyRecipes"
local_original_filename = "OC2ManyRecipes.dll"
install_target = "BepInEx/plugins"
enabled = true
```

GitHub 文件地址按 `https://github.com/{username}/{repository}/raw/refs/heads/{branch}/{file_path}` 拼接，Gitee 按 `https://gitee.com/{username}/{repository}/raw/{branch}/{file_path}` 拼接。文件模式不读取 Release，也不使用 Release 频道或资产匹配规则；安装文件名取相对路径末尾的文件名。

两种下载模式都可以填写 GUID、产品名称和原始文件名，供管理器识别已经安装的本地 MOD。仓库文件模式没有 Release 版本记录，因此不会检测 Release 更新；在 MOD 管理页显示“不是 Release 资产, 无法检测更新”。

仓库源不需要，也不应该添加下面这些导出配置包字段：

- `format`
- `schema_version`
- `exported_at`

这些字段只属于软件“导出配置”生成的完整配置包。完整配置包用于在另一台管理器中导入，不能直接替代仓库源文件。

仓库源可以使用 `.toml` 或 `.oc2repo.toml` 扩展名，例如 `repositories.toml` 或 `my-repositories.oc2repo.toml`。文件必须使用 UTF-8 编码；对外分享时，入口地址必须能够通过公开的 HTTP 或 HTTPS 地址返回 TOML 原文。

### 在仓库源中声明其他仓库源

仓库源文件可以使用 `[[repository_sources]]` 声明子仓库源。子仓库源会在父仓库源刷新成功后自动加入仓库管理，并继续递归刷新：

```toml
[[repository_sources]]
id = "azhe-hostutilities"
nickname = "阿哲 - 街机MOD"
url = "repositories-azhe-hostutilities.toml"

[repository_sources.nickname_i18n]
zh-CN = "阿哲 - 街机MOD"
zh-TW = "阿哲 - 街機MOD"
en = "Azhe - HostUtilities"
ko = "Azhe - 아케이드 MOD"

[[repository_sources]]
id = "gua"
nickname = "G_U_A"
url = "https://gitee.com/example/mod-releases/raw/main/repositories-gua.toml"
```

示例中的 `example` 是占位符，请替换为实际的仓库所有者。多语言表属于紧邻它之前的那个 `[[repository_sources]]`；声明下一个源时，需要重新写 `[[repository_sources]]`。

### 仓库源字段

| 字段 | 是否必填 | 说明 |
| --- | --- | --- |
| `id` | 是 | 源的唯一标识，同一源文件中不能重复。建议使用字母、数字、点、短横线和下划线，首字符使用字母或数字。 |
| `nickname` | 是 | 默认昵称；未配置当前语言翻译时显示此名称。 |
| `nickname_i18n` | 否 | 多语言昵称表，支持 `zh-CN`、`zh-TW`、`en`、`ko`。 |
| `url` | 是 | 完整 HTTP(S) 直链，或相对于当前源所在仓库的文件路径。 |
| `region` | 否 | `chinese_mainland` 仅中国大陆，`international` 仅非中国大陆；省略时对所有地区显示。 |
| `builtin` | 否 | 子源是否继承父源的内置保护，普通自定义源通常省略。使用布尔值 `true` / `false`。 |

`nickname_i18n` 根据当前界面语言选择昵称，翻译缺失或为空时使用 `nickname`。`region` 根据电脑地区设置筛选源，与界面语言无关：只想翻译同一个源的标题时，添加多语言表即可，不需要为了翻译重复声明源。同一 URL 可以按不同地区声明多个源，但每个源必须使用不同的 `id`。

`builtin` 不能把普通自定义源变成受保护的内置源。内置父源的子源默认继承保护；子源显式填写 `builtin = false` 后，该子源及其后代不再继承保护，后代即使填写 `builtin = true` 也不能重新开启保护。兼容已有的 `"true"` / `"false"` 字符串写法，但新文件建议使用布尔值。

嵌套源不需要填写 `parent_id`，管理器会根据声明它的父源维护父子关系。源文件成功解析后，管理器会先展示其中的仓库和子源，再分别刷新 Release 和子源内容；某一项失败不会阻止其他项展示。如果父源移除了某个子源，下次刷新也会移除该子源及其下级内容。不要让子源引用自身或循环引用父源。

### 子源地址和相对路径

`url` 填写完整 HTTP(S) 地址时，管理器直接请求该地址。填写文件路径时，按以下规则解析：

| `url` 写法 | 解析位置 |
| --- | --- |
| `mods/child.toml` | GitHub/Gitee raw 地址所属仓库的当前分支根目录。 |
| `./child.toml` | 当前源文件所在目录。 |
| `../shared.toml` | 当前源文件所在目录的上一级，不能越过仓库分支根目录。 |
| `https://example.com/child.toml` | 指定的完整直链。 |

例如，父源地址是 `https://gitee.com/owner/repo/raw/main/config/index.toml`：

- `url = "mods/child.toml"` 对应 `https://gitee.com/owner/repo/raw/main/mods/child.toml`。
- `url = "./child.toml"` 对应 `https://gitee.com/owner/repo/raw/main/config/child.toml`。
- `url = "../shared.toml"` 对应 `https://gitee.com/owner/repo/raw/main/shared.toml`。

GitHub 支持 `/raw/main/`、`/raw/refs/heads/master/` 和 `raw.githubusercontent.com` 形式，路径会保留当前分支或标签。普通 HTTP(S) 文件地址按当前文件目录解析相对路径。完整直链指向另一仓库后，该文件声明的后续子源也从新仓库解析。Raw 链接跳转到 CDN 时仍保留原始仓库和分支作为路径基准。

管理器自带的源也使用相同格式，普通文件路径从其资源根目录解析。维护一组源文件时，可以保留相同目录结构上传到线上，继续使用原来的相对路径；线上源不会通过相对路径读取用户电脑上的文件。

## 如何制作仓库源

### 1. 填写仓库身份

具体 MOD 仓库使用三个必填字段：

- `provider`：托管平台，填写 `github` 或 `gitee`。
- `username`：仓库所属的用户或组织名。
- `repository`：平台上的实际仓库名，不能用 MOD 的显示名称代替。

例如，项目位于 `https://github.com/gua248/Overcooked2-ManyRecipes` 时，填写：

```toml
provider = "github"
username = "gua248"
repository = "Overcooked2-ManyRecipes"
```

管理器按平台自动拼接项目主页：GitHub 使用 `https://github.com/{username}/{repository}`，Gitee 使用 `https://gitee.com/{username}/{repository}`。Release 模式据此请求对应 API；打开发布页时在主页后添加 `/releases`。不要在 `username`、`repository` 中填写完整地址、`/raw` 或 `/releases` 路径，也不要在 `[[repositories]]` 中添加 `url`。

若文件位于分支中，设置 `download_mode = "repository_file"`，并提供 `branch` 和 `file_path`。文件模式只支持具体文件路径，不支持通配符或正则；GitHub Raw 代理从管理器的全局 Raw 代理列表读取。仓库源与代理来源的 `url` 指向文档地址，仍按前述直链和相对路径规则填写。

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

单引号 TOML 字符串适合保存正则表达式，因为反斜杠不需要再次转义。正则表达式必须能够被 Rust `regex` 库解析，不能使用环视或反向引用等不支持的语法。

### 4. 设置安装目录

`install_target` 是相对于游戏目录的安装路径。在线安装以此字段为准，目标目录不存在时会创建；请根据发布文件的目录结构填写：

```toml
install_target = "BepInEx/plugins"
```

如果压缩包本身已经包含 `BepInEx` 顶层目录，可以设置为空字符串，让管理器按照压缩包内部结构安装：

```toml
install_target = ""
```

单独发布 DLL 时，通常填写 `BepInEx/plugins`。不要填写绝对路径，也不要使用 `..` 跳出游戏目录。

DLL、纯 DLL ZIP 和单个普通文件夹直接放入配置目录，保留 ZIP 内的普通文件夹名。例如 ZIP 只有 `OC2HostUtilities/` 文件夹，配置 `BepInEx/plugins` 后会得到 `BepInEx/plugins/OC2HostUtilities/`；不要在配置路径中再重复填写普通文件夹名。

混合 ZIP (文件夹和文件混合的ZIP) 会在配置路径下创建压缩包同名目录（去除 `.zip`），再将所有文件和目录按原结构放进去。例如 `HostUtilities.zip` 内有 `OCHostUtilities` 文件夹、DLL 和 README，配置 `BepInEx/plugins` 后全部放入 `BepInEx/plugins/HostUtilities/`。配置路径已指向同名包目录时 (`BepInEx/plugins/HostUtilities/`) 直接复用，避免多套一层。本地混合 ZIP 同样放入 `BepInEx/plugins/<ZIP 名称>/`。

ZIP 顶层仅有 `BepInEx/` 或 `plugins/` 时，管理器会合并其内容并替换同名文件，保留其他文件。`BepInEx/` 格式可使用空路径；`plugins/` 格式通常应配置 `BepInEx`。空路径始终表示游戏根目录，因此 `plugins/` 格式配空路径会安装到游戏根目录的 `plugins/`。目标已包含特殊目录前缀时会去除重复前缀，缺失目录自动创建；特殊目录旁有 README 等条目时，按普通混合 ZIP 完整安装。

在线重装或升级会先下载、校验新文件，再将旧安装清单对应的完整 MOD 包移入回收站（含禁用文件），最后安装新版。安装失败时尝试恢复旧包；其他包仍使用的共享文件不会移入回收站。

### 5. 填写本地 MOD 识别信息

Release 与仓库文件两种模式都使用 GUID、DLL 的 `ProductName` 和 `OriginalFilename` 将本地 MOD 绑定到仓库，三项中任意一项匹配即可；填写版本号正则时，还需满足版本条件。建议按实际 DLL 信息填写，不根据下载地址猜测：

```toml
plugin_guid = "com.example.my-mod"
local_product_name = "My MOD"
local_original_filename = "MyMod.dll"
```

如果插件没有 BepInEx GUID，保留 `plugin_guid = ""`，并至少填写 DLL 的产品名称或原始文件名。`plugin_guid` 负责识别本地插件，`asset_rules` 负责选择 Release 下载文件；仓库文件模式用 `file_path` 指定文件，不能用识别字段替代文件路径。

本地绑定不等于可以检测更新。只有 Release 模式会比较已安装版本与 Release 版本；仓库文件模式可以显示来源和识别信息，但不会请求 Release，也不会据此显示“最新版”或“有更新”。

### 6. 按当前 MOD 下载渠道展示仓库

每个 `[[repositories]]` 都可以填写 `region`，用法与 `[[repository_sources]]` 相同：

| 配置 | 展示渠道 |
| --- | --- |
| `region = "chinese_mainland"` | 仅中国大陆。 |
| `region = "international"` | 仅非中国大陆。 |
| 省略 `region` 或填写 `region = ""` | 所有地区。 |

管理器按照电脑地区设置选择当前 MOD 下载渠道，切换界面语言不会改变渠道。仓库必须同时满足自身和所属源的地区限制才会显示；例如，中国大陆源下的 `region = "international"` 仓库不会显示。想在同一源中提供国内和国际两组仓库时，源本身不要限定地区，再为各个仓库分别填写 `region`。

此字段控制仓库卡片展示、刷新、仓库统计、在线更新绑定和配置导出；不符合当前渠道的仓库不会参与这些操作，也不会删除已经安装的本地 MOD。修改仓库时，可以在二级菜单的“适用地区”中选择“中国大陆地区”“国际地区”或“全部地区”，新建仓库默认选择“全部地区”。

同名、同一 `provider` / `username` / `repository` 的仓库可以使用不同 `region` 分别声明，导入时不会因名称相同而互相覆盖。仓库字段写在所属 `[[repositories]]` 下，并放在 `name_i18n`、`group_i18n` 等子表之前。

### 7. 区分版本和限制 Release 数量

`channel` 根据 Release 是否标记为预发布选择版本：`all` 同时获取正式发布和预发布，`stable` 只取正式发布，`beta` 只取预发布。省略时及新建仓库时默认使用 `all`；草稿 Release 不会使用。该字段只影响 Release 模式，不用于仓库文件下载。

如果多个仓库使用同一 GUID，但对应不同版本系列，可以用 `version_regex` 进一步限制本地绑定。例如，只允许三段数字版本绑定：

```toml
version_regex = '^\d+\.\d+\.\d+$'
max_release_count = 10
```

填写正则后，本地版本必须匹配它，并且 GUID、产品名称、原始文件名三项中至少有一项匹配，才能绑定此仓库。正则为空或省略时，不额外限制版本格式。单引号可以避免反斜杠转义；不要仅通过版本位数猜测稳定版或测试版，应按自己实际的版本命名规则填写。

`max_release_count` 是当前仓库的配置，默认 10，可填写 1 到 100。每个 `[[repositories]]` 可以设置不同的数量，不会改变其他仓库的设置。

## 仓库源完整示例

下面的示例展示了分组、多语言名称、多语言简介、Markdown 简介和正则资产规则：

```toml
[[repositories]]
name = "BepInEx 配置管理器"
group = "前置必装"
provider = "github"
username = "BepInEx"
repository = "BepInEx.ConfigurationManager"
download_mode = "release"
channel = "all"
install_target = ""
note = """
此模组用于管理《主机实用MOD》等MOD创建的配置，其默认打开的快捷键是 F1，但可能经由《主机实用MOD》自动修改为 P。
"""
plugin_guid = "com.bepis.bepinex.configurationmanager"
local_product_name = "BepInEx.ConfigurationManager"
local_original_filename = "ConfigurationManager.dll"
max_release_count = 10
download_threads = 32
enabled = true

[repositories.name_i18n]
en = "BepInEx Configuration Manager"
ko = "BepInEx 구성 관리자"
zh-CN = "BepInEx 配置管理器"
zh-TW = "BepInEx 設定管理器"

[repositories.group_i18n]
en = "Required Prerequisites"
ko = "필수 선행 항목"
zh-CN = "前置必装"
zh-TW = "前置必裝"

[[repositories.asset_rules]]
type = "regex"
value = '^BepInEx\.ConfigurationManager_BepInEx5_v\d+\.\d+\.zip$'

[repositories.note_i18n]
en = "This mod is used to manage configurations created by mods such as HostUtilities. Its default hotkey is F1, but HostUtilities may automatically change it to P."
ko = "이 모드는 《HostUtilities》와 같은 MOD에서 생성한 설정을 관리하는 데 사용됩니다. 기본 단축키는 F1이지만, 《HostUtilities》에 의해 자동으로 P로 변경될 수 있습니다."
zh-CN = "此模组用于管理《主机实用MOD》等MOD创建的配置，其默认打开的快捷键是 F1，但可能经由《主机实用MOD》自动修改为 P。"
zh-TW = "此模組用於管理《主機實用MOD》等MOD建立的設定，預設開啟快捷鍵為 F1，但可能會由《主機實用MOD》自動修改為 P。"

[[repositories]]
name = "主机实用 MOD（街机 MOD）稳定版 "
group = "主机实用 MOD 稳定版"
provider = "gitee"
username = "ch3ngyz"
repository = "Overcooked-2-HostUtilities-Stable-Releases"
download_mode = "release"
region = "chinese_mainland"
channel = "all"
install_target = "BepInEx/plugins"
note = """
提供主机身份下的成员管理、语音聊天、文字聊天、黑名单管理、延迟查看、跳关、重开等提升《胡闹厨房2》游戏体验的辅助功能。
如果下载速度较慢或下载失败，也可以尝试 GitHub 渠道。
"""
plugin_guid = "com.ch3ngyz.plugin.HostUtilities"
local_product_name = "主机实用MOD (街机MOD)"
local_original_filename = "HostUtilities.dll"
max_release_count = 10
download_threads = 1
enabled = true

[repositories.name_i18n]
en = "HostUtilities (Arcade MOD) Stable - Gitee"
ko = "HostUtilities (아케이드 MOD) 안정 버전 - Gitee"
zh-CN = "主机实用 MOD（街机 MOD）稳定版 "
zh-TW = "主機實用 MOD（街機 MOD）穩定版 - Gitee 頻道"

[repositories.group_i18n]
en = "HostUtilities Stable"
ko = "HostUtilities 안정 버전"
zh-CN = "主机实用 MOD 稳定版"
zh-TW = "主機實用 MOD 穩定版"

[[repositories.asset_rules]]
type = "exact"
value = "HostUtilities.zip"

[repositories.note_i18n]
en = """
Provides player management as the host, voice chat, text chat, blacklist management, latency display, level skipping, restarting, and other features that enhance the Overcooked! 2 gaming experience.
If the download is slow or fails, the GitHub channel can also be used.
"""
ko = """
호스트 권한을 통한 멤버 관리, 음성 채팅, 문자 채팅, 블랙리스트 관리, 지연 시간 확인, 스테이지 건너뛰기, 재시작 등 《Overcooked! 2》의 게임 경험을 향상시키는 다양한 보조 기능을 제공합니다.
다운로드 속도가 느리거나 실패하면 GitHub 채널을 이용할 수도 있습니다.
"""
zh-CN = """
提供主机身份下的成员管理、语音聊天、文字聊天、黑名单管理、延迟查看、跳关、重开等提升《胡闹厨房2》游戏体验的辅助功能。
如果下载速度较慢或下载失败，也可以尝试 GitHub 渠道。
"""
zh-TW = """
提供主機身分下的成員管理、語音聊天、文字聊天、黑名單管理、延遲查看、跳關、重開等提升《胡鬧廚房2》遊戲體驗的輔助功能。
如果下載速度較慢或下載失敗，也可以嘗試 GitHub 頻道。
"""
```

可以参考 [仓库源入口示例](src-tauri/resources/repositories.toml) 和 [街机 MOD 仓库示例](src-tauri/resources/repositories-azhe-hostutilities.toml)。`[[repositories]]` 不需要手动填写 `id` 或 `source_id`，管理器会维护仓库标识和来源绑定；也可以直接复制软件导出的仓库条目，已有的 `source_id` 会被当前源的 ID 替换。

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

分组名称由 TOML 决定，管理器不会根据 GUID 或仓库名称自动猜测分组。没有填写 `group` 的仓库不显示分类标题。

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

## MOD 仓库字段参考

| 字段 | 是否必填 | 说明 |
| --- | --- | --- |
| `name` | 是 | 默认显示名称。 |
| `name_i18n` | 否 | 多语言名称表。 |
| `group` | 否 | 分组的稳定键；为空时不显示分类标题。 |
| `group_i18n` | 否 | 分组的多语言显示名称。 |
| `provider` | 是 | `github` 或 `gitee`。 |
| `username` | 是 | 平台上的仓库所属用户或组织名，不填写完整 URL。 |
| `repository` | 是 | 平台上的实际仓库名，不填写 `/raw`、`/releases` 或文件路径。 |
| `download_mode` | 否 | `release` 下载 Release 资产；`repository_file` 下载指定分支内的单个文件。省略时为 `release`。 |
| `branch` | 文件模式必填 | 仓库文件模式使用的分支名，例如 `main` 或 `master`。 |
| `file_path` | 文件模式必填 | 相对分支根目录的文件路径，例如 `bin/Release/Example.dll`。 |
| `region` | 否 | `chinese_mainland` 仅中国大陆，`international` 仅非中国大陆；省略或为空时对所有地区展示，还需满足所属源的地区限制。 |
| `channel` | 否 | 仅 Release 模式使用。`all`、`stable` 或 `beta`；省略时默认 `all`。正式版排除 prerelease，测试版只取 prerelease，全部版本包含两者；草稿 Release 不会使用。 |
| `asset_rules` | Release 模式必填 | 一个或多个 Release 资产规则，至少一条。多个规则满足任意一条即可。 |
| `install_target` | 是 | 游戏目录下的相对安装目录；空字符串表示按压缩包内部结构安装。 |
| `note` | 否 | 默认 Markdown 简介。 |
| `note_i18n` | 否 | 多语言 Markdown 简介表。 |
| `plugin_guid` | 否 | 两种下载模式均支持的本地 BepInPlugin GUID；省略或未知时可填写空字符串，并用产品名称或原始文件名识别。 |
| `local_product_name` | 否 | DLL 的 ProductName，用于本地绑定。 |
| `local_original_filename` | 否 | DLL 的 OriginalFilename，用于本地绑定。 |
| `version_regex` | 否 | 本地 MOD 版本正则；填写后只有版本匹配时才会绑定，留空则忽略版本条件。 |
| `max_release_count` | 否 | 当前仓库单次获取的最大 Release 数量，范围为 1 到 100，默认 10。 |
| `download_threads` | 否 | GitHub 下载线程范围为 1 到 32，默认 16；Gitee 强制单线程。 |
| `enabled` | 是 | `true` 显示并刷新，`false` 暂停该仓库。 |

仓库源中的每个 `[[repositories]]` 都可以直接来自软件的导出配置。导出内容里的 `source_id` 只代表导出时的本地来源，发布仓库源时不需要手动修改；管理器下载仓库源后会忽略这些旧值，并统一写入当前仓库源配置的唯一 ID。

## 代理配置建议

API、GitHub Release 和 GitHub Raw 文件的代理统一从管理器内的“GitHub 代理配置”管理，分别维护 GitHub API、Gitee API、Release 与 Raw 列表。仓库源不保存代理策略或代理 URL；GitHub 资产的安装菜单依次提供自动代理、自动直连、直链和手动选择。中国大陆默认自动代理，国际地区默认自动直连；Release 和仓库文件下载共用这套策略，Gitee 资产只直连。

自动直连先尝试直连，失败后按代理顺序继续。自动代理先尝试 JS 脚本来源，再尝试内置代理，全部代理失败后尝试直连。直链只请求原始地址，手动选择只请求所选启用代理。取消下载会停止后续尝试。

每个渠道的 Release API 按“官方直连 → 官方直连并携带 Token → 该渠道 API 代理”的顺序请求；前一步失败后才进入下一步。GitHub API 代理只接在 GitHub 请求之后，Gitee API 代理只接在 Gitee 请求之后；Token 始终只发送到官方 API，不会发送给代理。

内置仓库索引 `repositories.toml` 可以按地区声明多个代理来源。代理来源表只从这个内置索引读取，普通远程仓库源里的同名表不会自动作为全局代理配置加载。`url` 可以是 HTTPS 直链，也可以是相对于当前 `repositories.toml` 的路径；同仓库文件直接填写相对路径即可。`format` 可选 `toml` 或 `userscript`，默认 `toml`。多个来源都会读取并合并，重复代理模板会去重：

```toml
[[github_proxy_sources]]
region = "chinese_mainland"
url = "https://gitee.com/example/mod-index/raw/main/repositories-proxies.toml"

[[github_proxy_sources]]
region = "international"
url = "repositories-proxies.toml"

[[github_proxy_sources]]
region = "international"
format = "userscript"
url = "https://example.com/scripts/github-proxies.user.js"
```

`github_proxy_sources.region` 是可选的：填写 `chinese_mainland` 时只在中国大陆加载，填写 `international` 时只在国际渠道加载；省略 `region` 或填写空字符串时，两个渠道都会加载这个代理来源。来源文件内解析出的 Release、Raw 和 API 代理会按当前渠道合并，并继续按各代理条目的 `regions` 显示地区信息。

JS 脚本中的 `download_url_us` 和 `raw_url` 仅解析为数据，不执行脚本；这些代理显示“远程”标签，TOML 清单条目显示“内置”。Release 和 Raw 列表把远程代理放在内置代理前面，保留各组内的排序并按模板去重。脚本请求或解析失败、列表为空时，管理器仍保留内置代理作为后备。来源标签由管理器维护，制作者不需要填写 `builtin` 或 `remote`。

`repositories-proxies.toml` 的 `schema_version` 可省略，省略时默认为 1，目前支持版本 1。Release 和 Raw 下载代理分别使用 `[[release_proxies]]`、`[[raw_proxies]]`；API 代理按渠道分为 `[[github_api_proxies]]` 和 `[[gitee_api_proxies]]`。每项包含唯一 `id`、显示名称 `name`、安全模板 `template`、可选地区 `regions` 和说明 `description`。地区仅作界面说明，不会自动探测代理服务器的物理位置。

模板不会执行 Python 或其他表达式，只替换以下白名单占位符：`{url}`（目标完整 URL）、`{url_no_scheme}`、`{github_path}`、`{url_path}`（目标 URL 的路径和查询参数）、`{owner}`、`{repo}`、`{tag}`、`{asset}`、`{branch}` 和 `{path}`。例如 `https://proxy.example/{url}`、`https://oc2gtapi.example{url_path}` 或 `https://cdn.example/gh/{owner}/{repo}@{branch}/{path}`。模板必须是 HTTPS；代理列表中的重复模板会自动合并并去重。不要把 Token 写进代理 URL。

Gitee 通常使用单线程下载，GitHub 可以使用多线程下载。仓库源中的线程数只是下载设置，不能改变 Release API 的返回内容。

## 在管理器中添加仓库源

1. 将 `.toml` 或 `.oc2repo.toml` 文件上传到一个稳定的公开地址。
2. 用浏览器或 `curl` 确认地址返回的是 TOML 原文，而不是 HTML 页面、登录页或 JSON 错误。
3. 打开 OC2 MOD Manager 的“MOD安装”页面。
4. 打开“管理仓库源”，点击“添加仓库源”。
5. 填写仓库源昵称和 TOML 文件地址。
   新建仓库源时还要填写“仓库唯一 ID”。它只能包含字母、数字、点、短横线和下划线，且首字符必须是字母或数字，例如 `my-source-id`。
6. 点击“保存仓库源”。只有保存后，管理器才会请求仓库源文件。
7. 文件解析成功后，文件中的仓库会出现在对应的仓库源和分组中；文件中的 `[[repository_sources]]` 也会被加入并自动刷新其下级仓库源和 Release。

从软件导出仓库配置并粘贴到仓库源文件时，可以保留导出的 `source_id`，管理器会在下载时自动忽略它，并把所有仓库绑定到“仓库唯一 ID”对应的仓库源。这样同一份导出内容可以复制到不同仓库源，而不会继承旧的来源 ID。

刷新源时会先清理该源的旧内容。下载或解析失败后，错误会显示在对应源的加载区域，不添加该源下的仓库，也不会用其他源的内容替代。修正文件或地址后，点击该源的刷新按钮重试；其他已成功读取的源不受影响。

## 发布前检查清单

- [ ] 文件是 UTF-8 编码的 TOML。
- [ ] 顶层至少存在一个 `[[repositories]]` 或 `[[repository_sources]]`。
- [ ] 没有加入 `format`、`schema_version`、`exported_at` 等导出包字段；`[[repository_sources]]` 是仓库源文件允许使用的条目。
- [ ] 每个 MOD 仓库使用 `provider`、`username`、`repository`，用户名和仓库名与实际项目一致，且没有 `url` 字段。
- [ ] Release 模式仓库至少有一条有效的 `asset_rules`；仓库文件模式已填写有效的 `branch` 和 `file_path`。
- [ ] Release 模式的资产规则能匹配 Release 中真实存在的文件名；仓库文件模式的 `file_path` 指向分支中真实存在的文件。
- [ ] `plugin_guid`、`local_product_name`、`local_original_filename` 与 DLL 实际信息一致。
- [ ] `install_target` 是相对路径，没有绝对路径和 `..`。
- [ ] `[[repository_sources]]` 的 `id` 不重复，子源没有循环引用。
- [ ] 多语言源昵称、MOD 名称、分组和简介使用固定语言键，且对应的表放在正确的条目下。
- [ ] 需要按地区显示时，为仓库源或仓库填写 `region = "chinese_mainland"` 或 `region = "international"`；省略表示所有地区，仓库和所属源的限制必须同时满足。
- [ ] 相对路径对应当前仓库和分支中的实际文件，完整直链返回 TOML 原文。
- [ ] Release 模式的 `channel` 符合预期，省略时为 `all`；填写了 `version_regex` 时，正则能匹配预期的本地版本。
- [ ] 仓库文件模式已填写准确的本地识别字段，并知晓该模式不支持 Release 更新检测。
- [ ] `max_release_count` 在 1 到 100 之间，省略时使用 10。
- [ ] 简介中的 Markdown 使用三引号保存多行内容。
- [ ] URL 不包含 Token、密码或 Cookie。
- [ ] 用浏览器确认远程文件可以直接返回原始 TOML。
