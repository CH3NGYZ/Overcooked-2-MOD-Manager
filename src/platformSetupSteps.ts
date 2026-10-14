export interface PlatformSetupStep {
  title: string
  paragraphs: string[]
  images: string[]
}

// Existing CrossOver operations, split into one action per page.
export const crossoverSetupSteps: PlatformSetupStep[] = [
  {
    "title": "下载 UU 加速器",
    "paragraphs": [
      "下载 UU 加速器，使用手机号登录后加速 Steam 平台，之后放到后台即可。"
    ],
    "images": []
  },
  {
    "title": "下载 CrossOver",
    "paragraphs": [
      "加 QQ 群 1103401780，下载群文件 CrossOver，输入密码 digit77.com（输入两次）后打开安装页面。"
    ],
    "images": []
  },
  {
    "title": "安装 CrossOver",
    "paragraphs": [
      "将 CrossOver 拖到 Applications 文件夹，等待复制完毕。"
    ],
    "images": [
      "MacOS/Drag crossover to applications.png"
    ]
  },
  {
    "title": "准备 CROSLIC 文件夹",
    "paragraphs": [
      "将 CROSLIC 文件夹拖动到桌面。"
    ],
    "images": [
      "MacOS/Drag croslic folder to desktop.png"
    ]
  },
  {
    "title": "终端进入 CROSLIC 目录",
    "paragraphs": [
      "在启动台打开终端，输入 cd（空格），然后将桌面的 croslic 文件夹拖入终端，最后回车。"
    ],
    "images": [
      "MacOS/Drag croslic folder to terminal.png"
    ]
  },
  {
    "title": "执行授权工具",
    "paragraphs": [
      "打开 croslic 文件夹，将 license.tool 文件拖到终端并回车。"
    ],
    "images": [
      "MacOS/Drag license.tool to Terminal.png"
    ]
  },
  {
    "title": "输入电脑密码",
    "paragraphs": [
      "按终端提示输入电脑密码；输入时不会显示字符，这是正常现象。"
    ],
    "images": []
  },
  {
    "title": "确认操作完成",
    "paragraphs": [
      "终端出现 \"replacing existing signature\" 后，确认操作完成，再关闭终端和文件夹。"
    ],
    "images": [
      "MacOS/Appear replacing existing signature.png"
    ]
  },
  {
    "title": "打开 CrossOver",
    "paragraphs": [
      "打开 CrossOver。若出现无法打开的提示，点击“完成”，继续下一步；若能正常打开，可跳到下载 Windows 版 Steam。"
    ],
    "images": [
      "MacOS/Can't open crossover.png"
    ]
  },
  {
    "title": "打开隐私与安全性",
    "paragraphs": [
      "去\"设置\"打开\"隐私与安全性\""
    ],
    "images": []
  },
  {
    "title": "允许运行 CrossOver",
    "paragraphs": [
      "滑到下方，针对 CrossOver 点击\"仍要允许\"并输入 Touch ID 或密码"
    ],
    "images": [
      "MacOS/Use Touch ID or password to open Crossover.png"
    ]
  },
  {
    "title": "下载 Windows 版 Steam",
    "paragraphs": [
      "打开 CrossOver 后，先下载 Windows 版 Steam 安装包：https://store.steampowered.com，选择 Windows 平台。下载后备用。"
    ],
    "images": []
  },
  {
    "title": "在 CrossOver 中安装 Steam",
    "paragraphs": [
      "切换到 CrossOver 主界面，右上角点击\"安装一个不在列表里的应用程序\"。"
    ],
    "images": [
      "MacOS/Install an application.png"
    ]
  },
  {
    "title": "选择 Steam 安装包",
    "paragraphs": [
      "在\"详情\"的\"提供安装程序\"处，选择右侧的\"编辑\"，选择刚下载的 SteamSetup.exe。"
    ],
    "images": [
      "MacOS/Details select steam setup.png"
    ]
  },
  {
    "title": "创建容器",
    "paragraphs": [
      "在\"安装容器\"处点击\"编辑\"，选择\"新建容器\"，命名为\"厨房\"，类型选 Windows 10。"
    ],
    "images": [
      "MacOS/Create a bottle.png"
    ]
  },
  {
    "title": "开始安装 Steam",
    "paragraphs": [
      "点击 CrossOver 右上角“安装”，授予必要权限，在 Steam 安装界面按提示点击“下一步”。"
    ],
    "images": []
  },
  {
    "title": "完成 Steam 安装",
    "paragraphs": [
      "Steam 安装完成时，取消勾选“运行 Steam”，再点击“完成”。"
    ],
    "images": []
  },
  {
    "title": "安装应用到容器",
    "paragraphs": [
      "容器和 Steam 已安装完毕。选择右侧\"安装应用程序到容器\"按钮。"
    ],
    "images": [
      "MacOS/Install an application to Bottles.png"
    ]
  },
  {
    "title": "安装中文支持",
    "paragraphs": [
      "在左上角搜索“中文”，安装将容器语言切换到简体中文的软件。"
    ],
    "images": []
  },
  {
    "title": "安装微软雅黑",
    "paragraphs": [
      "再次选择“安装应用程序到容器”，搜索并安装“微软雅黑”，使用同一个“厨房”容器。"
    ],
    "images": []
  },
  {
    "title": "配置 Wine",
    "paragraphs": [
      "所有软件安装完毕后，在容器界面选择\"Wine 配置\"。"
    ],
    "images": [
      "MacOS/Click Wine configuration.png"
    ]
  },
  {
    "title": "添加 winhttp 库",
    "paragraphs": [
      "切换到“函数库”，在下拉列表选择“winhttp”，点击右侧“添加”。"
    ],
    "images": [
      "MacOS/Execute Wine configuration.png"
    ]
  },
  {
    "title": "确认 winhttp 覆盖",
    "paragraphs": [
      "确认 winhttp 已在列表内，并设为“原装先于内建”（native, builtin），点击“应用”及“确定”，关闭 Wine 配置。"
    ],
    "images": []
  },
  {
    "title": "下载游戏",
    "paragraphs": [
      "回到“厨房”容器，运行其中的 Windows Steam，登录并下载《胡闹厨房2》。"
    ],
    "images": []
  },
  {
    "title": "检测厨房目录",
    "paragraphs": [
      "下载完毕后，在管理器中刷新游戏目录。成功获取目录后，即可安装 BepInEx 框架和 MOD。"
    ],
    "images": []
  }
]
