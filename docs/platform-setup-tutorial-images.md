# CrossOver 设置向导与图片清单

当前向导位于 `src/components/PlatformSetupGuide.vue`，步骤数据位于 `src/platformSetupSteps.ts`。仅在 macOS Wine 模式显示，Windows、Linux 和 macOS 原生模式不显示此向导；完整 Linux 教程后续制作。

## 弹窗与导航

- 弹窗 Teleport 到 body，复用全局按 `--ui-zoom` 缩放的逻辑画布；宽度不超过画布 90%，高度为 90%，因此实际显示不超过窗口宽高 90%。不能再使用未换算缩放的 `90vh` 限高。
- 标题及底部上一步/下一步固定占位；中间正文设置 `min-height: 0` 和纵向滚动，图片保持比例，长图可上下查看。
- 一次显示一个步骤，共 25 页；翻页后正文回到顶部并把键盘焦点移到步骤标题。首步禁用上一步，末步显示“我已完成所需操作”。完成事件及父组件的关闭/目录重检入口沿用原有连接。
- 关闭再打开从第一步开始；Esc 关闭，Tab 留在弹窗内，关闭后恢复入口焦点。拖动正文后在遮罩释放鼠标不误关闭。
- 文案支持简中、繁中、英文、韩文；图片仍使用现有界面截图，不假设已有其他语言截图。

## 实际资源路径

现有 13 张截图存放在 `src/assets/tutorial/MacOS/`，通过 Vite 的 `import.meta.glob` 加载；并不存在旧说明中的 `src/assets/platform-tutorials/` 运行时映射。新图也放到前者，在对应步骤的 `images` 数组登记完整 `MacOS/文件名.png`。没有图片的步骤继续显示说明，不显示空白占位或失效图标。

| 步骤 | 操作 | 当前图片 |
| --- | --- | --- |
| 1 | 下载 UU 加速器 | 待补图，当前显示文字 |
| 2 | 下载 CrossOver | 待补图，当前显示文字 |
| 3 | 安装 CrossOver | `Drag crossover to applications.png` |
| 4 | 准备 CROSLIC 文件夹 | `Drag croslic folder to desktop.png` |
| 5 | 终端进入 CROSLIC 目录 | `Drag croslic folder to terminal.png` |
| 6 | 执行授权工具 | `Drag license.tool to Terminal.png` |
| 7 | 输入电脑密码 | 待补图，当前显示文字 |
| 8 | 确认操作完成 | `Appear replacing existing signature.png` |
| 9 | 打开 CrossOver | `Can't open crossover.png` |
| 10 | 打开隐私与安全性 | 待补图，当前显示文字 |
| 11 | 允许运行 CrossOver | `Use Touch ID or password to open Crossover.png` |
| 12 | 下载 Windows 版 Steam | 待补图，当前显示文字 |
| 13 | 在 CrossOver 中安装 Steam | `Install an application.png` |
| 14 | 选择 Steam 安装包 | `Details select steam setup.png` |
| 15 | 创建容器 | `Create a bottle.png` |
| 16 | 开始安装 Steam | 待补图，当前显示文字 |
| 17 | 完成 Steam 安装 | 待补图，当前显示文字 |
| 18 | 安装应用到容器 | `Install an application to Bottles.png` |
| 19 | 安装中文支持 | 待补图，当前显示文字 |
| 20 | 安装微软雅黑 | 待补图，当前显示文字 |
| 21 | 配置 Wine | `Click Wine configuration.png` |
| 22 | 添加 winhttp 库 | `Execute Wine configuration.png` |
| 23 | 确认 winhttp 覆盖 | 待补图，当前显示文字 |
| 24 | 下载游戏 | 待补图，当前显示文字 |
| 25 | 检测厨房目录 | 待补图，当前显示文字 |

其中 `Appear replacing existing signature.png` 原来已有但未在向导显示，现用于第 8 步。第 22 步截图只展示添加 winhttp，尚不能证明第 23 步已设为原装先于内建。

## 缺少的截图

这些截图不是完成本轮布局和分步功能的前置条件。优先补充容易操作错误的页面：

| 步骤 | 优先级 | 需要画面 |
| --- | --- | --- |
| 1 | 可选 | UU 加速器登录后选中 Steam 并开启加速 |
| 2 | 可选 | CrossOver 下载文件和安装页面（不包含下载密码或私人群聊） |
| 7 | 可选 | 终端等待输入电脑密码的状态（不要展示实际密码） |
| 10 | 优先 | 从系统设置进入“隐私与安全性”的入口 |
| 12 | 优先 | Steam 官网选择下载 Windows 版 SteamSetup.exe |
| 16 | 可选 | CrossOver 点击安装后出现的 Steam 安装第一页 |
| 17 | 优先 | Steam 安装完成页，标出取消勾选“运行 Steam” |
| 19 | 优先 | CrossOver 搜索并安装中文支持，显示目标“厨房”容器 |
| 20 | 优先 | CrossOver 搜索并安装微软雅黑，显示同一个容器 |
| 23 | 优先 | Wine 函数库列表中的 winhttp (native, builtin) 与应用按钮 |
| 24 | 可选 | CrossOver 厨房容器内启动 Windows Steam 并显示游戏库 |
| 25 | 可选 | 管理器检测成功的 Wine 厨房目录及框架安装入口 |

截图使用 PNG/JPG/WebP，优先清晰原图（建议宽度 1000–1600 像素），只保留当前操作区域，用箭头或框标出点击位置。不要包含账号、实际电脑密码、私人群聊或令牌；无需为了满足固定文件大小而把文字压糊。

## 验证边界

本轮应在真实浏览器加载实际 Vue 组件及生产样式，验证 1280×720、1920×1080 在 1.0/1.5/2.0 缩放下的 90% 尺寸、正文上下滚动、固定按钮、25 步导航、翻页滚动重置、完成/关闭/焦点恢复和平台门控。浏览器验证不代表 macOS WKWebView、CrossOver 安装或真实游戏启动已经验证。
