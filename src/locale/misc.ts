import type { DomainMessages } from './types'

/**
 * 剩余的小域：头像、仓库说明、Wine、教程、工具页、锁与日志。
 *
 * 每个域的条目都很少，分开建文件只会让 `index.ts` 多出一堆 import，
 * 所以放一起；组装时仍按 tag 第一段拆回各自的域。
 */

/** 头像加载（`avatar.*`）。 */
export const avatar: DomainMessages = {
  cachePathInvalid: {
    'zh-CN': '头像缓存路径无效',
    'zh-TW': '頭像快取路徑無效',
    en: 'Invalid avatar cache path',
    ko: '아바타 캐시 경로가 올바르지 않습니다',
    ja: 'アバターキャッシュのパスが無効です',
  },
  emptyContent: {
    'zh-CN': '头像内容为空',
    'zh-TW': '頭像內容為空',
    en: 'The avatar content is empty',
    ko: '아바타 내용이 비어 있습니다',
    ja: 'アバターの内容が空です',
  },
  noSource: {
    'zh-CN': '没有可用的头像下载源',
    'zh-TW': '沒有可用的頭像下載來源',
    en: 'No avatar download source is available',
    ko: '사용할 수 있는 아바타 다운로드 소스가 없습니다',
    ja: '利用できるアバターのダウンロード元がありません',
  },
  notImage: {
    'zh-CN': '下载到的内容不是图片',
    'zh-TW': '下載到的內容不是圖片',
    en: 'The downloaded content is not an image',
    ko: '다운로드한 내용이 이미지가 아닙니다',
    ja: 'ダウンロードした内容が画像ではありません',
  },
  ownerRequired: {
    'zh-CN': '头像需要仓库 owner',
    'zh-TW': '頭像需要倉庫 owner',
    en: 'An avatar requires a repository owner',
    ko: '아바타에는 저장소 owner가 필요합니다',
    ja: 'アバターにはリポジトリの owner が必要です',
  },
  readTimeout: {
    'zh-CN': '读取头像超时',
    'zh-TW': '讀取頭像逾時',
    en: 'Timed out reading the avatar',
    ko: '아바타 읽기 시간이 초과되었습니다',
    ja: 'アバターの読み込みがタイムアウトしました',
  },
  requestTimeout: {
    'zh-CN': '头像请求超时',
    'zh-TW': '頭像請求逾時',
    en: 'Avatar request timed out',
    ko: '아바타 요청 시간이 초과되었습니다',
    ja: 'アバターの要求がタイムアウトしました',
  },
  waitTimeout: {
    'zh-CN': '等待头像下载超时',
    'zh-TW': '等待頭像下載逾時',
    en: 'Timed out waiting for the avatar download',
    ko: '아바타 다운로드 대기 시간이 초과되었습니다',
    ja: 'アバターのダウンロード待ちがタイムアウトしました',
  },
  tooLarge: {
    'zh-CN': '头像超过大小上限：{bytes} 字节',
    'zh-TW': '頭像超過大小上限：{bytes} 位元組',
    en: 'The avatar exceeds the size limit: {bytes} bytes',
    ko: '아바타가 크기 제한을 초과했습니다: {bytes} 바이트',
    ja: 'アバターがサイズ上限を超えています: {bytes} バイト',
  },
  downloadFailed: {
    'zh-CN': '头像下载失败',
    'zh-TW': '頭像下載失敗',
    en: 'Failed to download the avatar',
    ko: '아바타 다운로드에 실패했습니다',
    ja: 'アバターのダウンロードに失敗しました',
  },
  clearFailed: {
    'zh-CN': '头像缓存清理失败',
    'zh-TW': '頭像快取清理失敗',
    en: 'Failed to clear the avatar cache',
    ko: '아바타 캐시 정리에 실패했습니다',
    ja: 'アバターキャッシュの消去に失敗しました',
  },
}

/** 仓库说明（`readme.*`）。 */
export const readme: DomainMessages = {
  cancelled: {
    'zh-CN': '仓库说明读取已取消',
    'zh-TW': '倉庫說明讀取已取消',
    en: 'Reading the repository description was cancelled',
    ko: '저장소 설명 읽기가 취소되었습니다',
    ja: 'リポジトリ説明の読み込みを中止しました',
  },
  defaultBranchUnknown: {
    'zh-CN': '无法确定仓库默认分支',
    'zh-TW': '無法確定倉庫預設分支',
    en: 'Could not determine the repository default branch',
    ko: '저장소 기본 브랜치를 확인할 수 없습니다',
    ja: 'リポジトリの既定ブランチを特定できません',
  },
  fileMissing: {
    'zh-CN': '文件不存在',
    'zh-TW': '檔案不存在',
    en: 'The file does not exist',
    ko: '파일이 없습니다',
    ja: 'ファイルが存在しません',
  },
  httpStatus: {
    'zh-CN': '仓库说明下载失败：HTTP {status}',
    'zh-TW': '倉庫說明下載失敗：HTTP {status}',
    en: 'Failed to download the repository description: HTTP {status}',
    ko: '저장소 설명 다운로드 실패: HTTP {status}',
    ja: 'リポジトリ説明のダウンロードに失敗しました: HTTP {status}',
  },
  invalidRequestId: {
    'zh-CN': '仓库说明请求标识无效',
    'zh-TW': '倉庫說明請求識別碼無效',
    en: 'Invalid repository description request ID',
    ko: '저장소 설명 요청 식별자가 올바르지 않습니다',
    ja: 'リポジトリ説明リクエストの識別子が無効です',
  },
  invalidUtf8: {
    'zh-CN': '仓库说明不是有效的 UTF-8 文本',
    'zh-TW': '倉庫說明不是有效的 UTF-8 文字',
    en: 'The repository description is not valid UTF-8 text',
    ko: '저장소 설명이 올바른 UTF-8 텍스트가 아닙니다',
    ja: 'リポジトリ説明が有効な UTF-8 テキストではありません',
  },
  loading: {
    'zh-CN': '正在读取仓库说明',
    'zh-TW': '正在讀取倉庫說明',
    en: 'Reading the repository description',
    ko: '저장소 설명을 읽는 중',
    ja: 'リポジトリ説明を読み込んでいます',
  },
  tooLarge: {
    'zh-CN': '仓库说明超过 2 MiB，无法展示',
    'zh-TW': '倉庫說明超過 2 MiB，無法顯示',
    en: 'The repository description exceeds 2 MiB and cannot be shown',
    ko: '저장소 설명이 2 MiB를 초과하여 표시할 수 없습니다',
    ja: 'リポジトリ説明が 2 MiB を超えているため表示できません',
  },
  tooManyRequests: {
    'zh-CN': '仓库说明请求过多，请稍后重试',
    'zh-TW': '倉庫說明請求過多，請稍後重試',
    en: 'Too many repository description requests. Try again later.',
    ko: '저장소 설명 요청이 너무 많습니다. 나중에 다시 시도하세요.',
    ja: 'リポジトリ説明の要求が多すぎます。後で再試行してください。',
  },
  readFailed: {
    'zh-CN': '仓库简介读取失败',
    'zh-TW': '倉庫簡介讀取失敗',
    en: 'Failed to read the repository description',
    ko: '저장소 소개를 읽지 못했습니다',
    ja: 'リポジトリ説明の読み込みに失敗しました',
  },
  timeout: {
    'zh-CN': '仓库简介请求超时',
    'zh-TW': '倉庫簡介請求逾時',
    en: 'The repository description request timed out',
    ko: '저장소 소개 요청 시간이 초과되었습니다',
    ja: 'リポジトリ説明の要求がタイムアウトしました',
  },
}

/** Wine / CrossOver（`wine.*`）。 */
export const wine: DomainMessages = {
  crossOverMissing: {
    'zh-CN': '请先安装 CrossOver，再启动 Wine 版厨房',
    'zh-TW': '請先安裝 CrossOver，再啟動 Wine 版廚房',
    en: 'Install CrossOver before launching the Wine build',
    ko: 'Wine 버전 주방을 실행하려면 먼저 CrossOver를 설치하세요',
    ja: 'Wine 版のキッチンを起動する前に CrossOver をインストールしてください',
  },
  prefixInvalid: {
    'zh-CN': 'Wine 容器路径无效',
    'zh-TW': 'Wine 容器路徑無效',
    en: 'Invalid Wine prefix path',
    ko: 'Wine 컨테이너 경로가 올바르지 않습니다',
    ja: 'Wine プレフィックスのパスが無効です',
  },
  prefixUnknown: {
    'zh-CN': '无法确定游戏使用的 Wine 容器，请重新选择厨房目录',
    'zh-TW': '無法確定遊戲使用的 Wine 容器，請重新選擇廚房目錄',
    en: 'Could not determine the Wine prefix used by the game. Select the game directory again.',
    ko: '게임이 사용하는 Wine 컨테이너를 확인할 수 없습니다. 주방 디렉터리를 다시 선택하세요.',
    ja: 'ゲームが使用する Wine プレフィックスを特定できません。キッチンディレクトリを選び直してください。',
  },
  steamMissing: {
    'zh-CN': 'Wine 容器中未找到 Windows Steam，请先完成安装',
    'zh-TW': 'Wine 容器中找不到 Windows Steam，請先完成安裝',
    en: 'Windows Steam was not found in the Wine prefix. Finish installing it first.',
    ko: 'Wine 컨테이너에 Windows Steam이 없습니다. 먼저 설치를 완료하세요.',
    ja: 'Wine プレフィックスに Windows 版 Steam がありません。先にインストールを完了してください。',
  },
  winhttpFailed: {
    'zh-CN': '无法自动配置 Wine winhttp，请在目标容器的 winecfg 中手动设置',
    'zh-TW': '無法自動設定 Wine winhttp，請在目標容器的 winecfg 中手動設定',
    en: 'Could not configure Wine winhttp automatically. Set it manually in winecfg of the target prefix.',
    ko: 'Wine winhttp를 자동으로 설정하지 못했습니다. 대상 컨테이너의 winecfg에서 직접 설정하세요.',
    ja: 'Wine の winhttp を自動設定できませんでした。対象プレフィックスの winecfg で手動設定してください。',
  },
  winhttpTimeout: {
    'zh-CN': 'Wine winhttp 自动配置超时，请在目标容器的 winecfg 中手动设置',
    'zh-TW': 'Wine winhttp 自動設定逾時，請在目標容器的 winecfg 中手動設定',
    en: 'Configuring Wine winhttp timed out. Set it manually in winecfg of the target prefix.',
    ko: 'Wine winhttp 자동 설정 시간이 초과되었습니다. 대상 컨테이너의 winecfg에서 직접 설정하세요.',
    ja: 'Wine winhttp の自動設定がタイムアウトしました。対象プレフィックスの winecfg で手動設定してください。',
  },
  prefixUnavailable: {
    'zh-CN': '游戏使用的 Wine 容器不可用',
    'zh-TW': '遊戲使用的 Wine 容器無法使用',
    en: 'The Wine prefix used by the game is unavailable',
    ko: '게임이 사용하는 Wine 컨테이너를 사용할 수 없습니다',
    ja: 'ゲームが使用する Wine プレフィックスを利用できません',
  },
}

/** 工具页动作（`tools.*`）。 */
export const toolsBackend: DomainMessages = {
  backupDirOpened: {
    'zh-CN': '已打开备份目录',
    'zh-TW': '已開啟備份目錄',
    en: 'Opened the backup directory',
    ko: '백업 디렉터리를 열었습니다',
    ja: 'バックアップディレクトリを開きました',
  },
  cacheCleared: {
    'zh-CN': '已清除缓存并重置界面',
    'zh-TW': '已清除快取並重設介面',
    en: 'Cache cleared and the interface reset',
    ko: '캐시를 지우고 인터페이스를 초기화했습니다',
    ja: 'キャッシュを消去し、画面をリセットしました',
  },
  devConsoleOpened: {
    'zh-CN': '开发者控制台已打开',
    'zh-TW': '開發人員主控台已開啟',
    en: 'Developer console opened',
    ko: '개발자 콘솔을 열었습니다',
    ja: '開発者コンソールを開きました',
  },
  gameDirOpened: {
    'zh-CN': '已打开游戏目录',
    'zh-TW': '已開啟遊戲目錄',
    en: 'Opened the game directory',
    ko: '게임 디렉터리를 열었습니다',
    ja: 'ゲームディレクトリを開きました',
  },
  modsDirOpened: {
    'zh-CN': '已打开模组目录',
    'zh-TW': '已開啟模組目錄',
    en: 'Opened the mods directory',
    ko: '모드 디렉터리를 열었습니다',
    ja: 'MOD ディレクトリを開きました',
  },
}

/** 改多彩名（`rename.*`）。 */
export const renameBackend: DomainMessages = {
  pushed: {
    'zh-CN': '已推送到街机MOD',
    'zh-TW': '已推送到主機實用工具',
    en: 'Pushed to HostUtilities',
    ko: '호스트 유틸리티로 전송했습니다',
    ja: 'ホストユーティリティに送信しました',
  },
}

/** 操作锁、日志截断、网络客户端创建。 */
export const lock: DomainMessages = {
  poisoned: {
    'zh-CN': '操作锁异常',
    'zh-TW': '操作鎖異常',
    en: 'Operation lock error',
    ko: '작업 잠금 오류',
    ja: '操作ロックの異常',
  },
}

export const log: DomainMessages = {
  frontendTruncated: {
    'zh-CN': '{text}...[前端日志已截断]',
    'zh-TW': '{text}...[前端日誌已截斷]',
    en: '{text}...[frontend log truncated]',
    ko: '{text}...[프런트엔드 로그 잘림]',
    ja: '{text}...[フロントエンドログは切り詰められました]',
  },
  frontendEmpty: {
    'zh-CN': '前端日志为空',
    'zh-TW': '前端日誌為空',
    en: 'The frontend log is empty',
    ko: '프런트엔드 로그가 비어 있습니다',
    ja: 'フロントエンドログが空です',
  },
  // 未知的日志类型
  unknownKind: {
    'zh-CN': '未知的日志类型',
    'zh-TW': '未知的日誌類型',
    en: 'Unknown log type',
    ko: '알 수 없는 로그 유형',
    ja: '不明なログ種別',
  },
}

/** `error` 域里剩下的几个（框架安装回滚）。 */
export const errorExtra: DomainMessages = {
  'framework.installFailedRollbackIncomplete': {
    'zh-CN': '安装失败，且回滚未完成，请手动检查 BepInEx 目录',
    'zh-TW': '安裝失敗，且回復未完成，請手動檢查 BepInEx 目錄',
    en: 'Installation failed and the rollback did not finish. Check the BepInEx directory manually.',
    ko: '설치에 실패했고 롤백도 완료되지 않았습니다. BepInEx 디렉터리를 직접 확인하세요.',
    ja: 'インストールに失敗し、ロールバックも完了していません。BepInEx ディレクトリを手動で確認してください。',
  },
}

/** 网络客户端（`network.*`）。 */
export const networkClient: DomainMessages = {
  clientCreateFailed: {
    'zh-CN': '创建 HTTP 客户端失败: {reason}',
    'zh-TW': '建立 HTTP 用戶端失敗: {reason}',
    en: 'Failed to create the HTTP client: {reason}',
    ko: 'HTTP 클라이언트 생성 실패: {reason}',
    ja: 'HTTP クライアントの作成に失敗しました: {reason}',
  },
}
