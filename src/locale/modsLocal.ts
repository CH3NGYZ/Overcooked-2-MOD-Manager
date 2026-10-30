import type { DomainMessages } from './types'

/**
 * modsLocal 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const modsLocal: DomainMessages = {
  // 名称
  name: {
    'zh-CN': '名称',
    'zh-TW': '名稱',
    en: 'Name',
    ko: '이름',
    ja: '名前',
  },
  // 卡片
  cards: {
    'zh-CN': '卡片',
    'zh-TW': '卡片',
    en: 'Cards',
    ko: '카드',
    ja: 'カード',
  },
  // 横排
  rows: {
    'zh-CN': '横排',
    'zh-TW': '橫排',
    en: 'Rows',
    ko: '행',
    ja: '行',
  },
  // 尚未安装任何 MOD
  noModsInstalled: {
    'zh-CN': '尚未安装任何 MOD',
    'zh-TW': '尚未安裝任何 MOD',
    en: 'No MODs installed',
    ko: '설치된 MOD 없음',
    ja: 'MODは未インストールです',
  },
  // 有更新
  update: {
    'zh-CN': '有更新',
    'zh-TW': '有更新',
    en: 'Update',
    ko: '업데이트',
    ja: '更新あり',
  },
  // 最新版
  latest: {
    'zh-CN': '最新版',
    'zh-TW': '最新版',
    en: 'Latest',
    ko: '최신',
    ja: '最新',
  },
  // 详情
  details: {
    'zh-CN': '详情',
    'zh-TW': '詳情',
    en: 'Details',
    ko: '상세',
    ja: '詳細',
  },
  // 收起
  collapse: {
    'zh-CN': '收起',
    'zh-TW': '收起',
    en: 'Collapse',
    ko: '접기',
    ja: '折りたたむ',
  },
  // 无更新
  upToDate: {
    'zh-CN': '无更新',
    'zh-TW': '無更新',
    en: 'Up to date',
    ko: '최신 상태',
    ja: '最新です',
  },
  // 未绑定
  unbound: {
    'zh-CN': '未绑定',
    'zh-TW': '未綁定',
    en: 'Unbound',
    ko: '미연결',
    ja: '未リンク',
  },
  // 仓库缺失
  repositoryMissing: {
    'zh-CN': '仓库缺失',
    'zh-TW': '倉庫缺失',
    en: 'Repository missing',
    ko: '저장소 없음',
    ja: 'リポジトリなし',
  },
  // 重复
  duplicate: {
    'zh-CN': '重复',
    'zh-TW': '重複',
    en: 'Duplicate',
    ko: '중복',
    ja: '重複',
  },
  // 旧副本
  oldCopy: {
    'zh-CN': '旧副本',
    'zh-TW': '舊副本',
    en: 'Old copy',
    ko: '이전 사본',
    ja: '古いコピー',
  },
  // 加载警告
  loadWarning: {
    'zh-CN': '加载警告',
    'zh-TW': '載入警告',
    en: 'Load warning',
    ko: '로드 경고',
    ja: '読み込み警告',
  },
  // 已禁用
  disabled: {
    'zh-CN': '已禁用',
    'zh-TW': '已停用',
    en: 'Disabled',
    ko: '비활성',
    ja: '無効',
  },
  // GUID：{guid}
  guid: {
    'zh-CN': 'GUID：{guid}',
    'zh-TW': 'GUID：{guid}',
    en: 'GUID: {guid}',
    ko: 'GUID: {guid}',
    ja: 'GUID: {guid}',
  },
  // 文件：{file}
  file: {
    'zh-CN': '文件：{file}',
    'zh-TW': '檔案：{file}',
    en: 'File: {file}',
    ko: '파일: {file}',
    ja: 'ファイル: {file}',
  },
  // MOD 包：{name}（{count} 个）
  modPackage: {
    'zh-CN': 'MOD 包：{name}（{count} 个）',
    'zh-TW': 'MOD 包：{name}（{count} 個）',
    en: 'MOD package: {name} ({count})',
    ko: 'MOD 패키지: {name}({count}개)',
    ja: 'MOD パッケージ: {name}（{count} 件）',
  },
  // 警告：{warning}
  warning: {
    'zh-CN': '警告：{warning}',
    'zh-TW': '警告：{warning}',
    en: 'Warning: {warning}',
    ko: '경고: {warning}',
    ja: '警告: {warning}',
  },
  // 删除 MOD
  deleteMod: {
    'zh-CN': '删除 MOD',
    'zh-TW': '刪除 MOD',
    en: 'Delete MOD',
    ko: 'MOD 삭제',
    ja: 'MOD を削除',
  },
  // 「{name}」将被移入回收站，可在回收站中恢复。继续？
  willBeMovedToTheRecycleBinAndCanBeRestoredContinue: {
    'zh-CN': '「{name}」将被移入回收站，可在回收站中恢复。继续？',
    'zh-TW': '「{name}」將被移入資源回收筒，可在資源回收筒中還原。繼續？',
    en: '"{name}" will be moved to the recycle bin and can be restored. Continue?',
    ko: '"{name}"을(를) 휴지통으로 옮깁니다. 복원할 수 있습니다. 계속할까요?',
    ja: '「{name}」をゴミ箱へ移動します（復元可能）。続行しますか？',
  },
  // 移入回收站
  moveToRecycleBin: {
    'zh-CN': '移入回收站',
    'zh-TW': '移入資源回收筒',
    en: 'Move to recycle bin',
    ko: '휴지통으로 이동',
    ja: 'ゴミ箱へ移動',
  },
  // 「{name}」将被直接删除，无法恢复。继续？
  willBeDeletedPermanentlyThisCannotBeUndoneContinue: {
    'zh-CN': '「{name}」将被直接删除，无法恢复。继续？',
    'zh-TW': '「{name}」將被直接刪除，無法還原。繼續？',
    en: '"{name}" will be deleted permanently. This cannot be undone. Continue?',
    ko: '"{name}"을(를) 바로 삭제합니다. 복원할 수 없습니다. 계속할까요?',
    ja: '「{name}」を直接削除します。元に戻せません。続行しますか？',
  },
  // 直接删除
  deletePermanently: {
    'zh-CN': '直接删除',
    'zh-TW': '直接刪除',
    en: 'Delete permanently',
    ko: '바로 삭제',
    ja: '直接削除',
  },
  // 该 MOD 属于 MOD 包
  thisModBelongsToAPackage: {
    'zh-CN': '该 MOD 属于 MOD 包',
    'zh-TW': '該 MOD 屬於 MOD 包',
    en: 'This MOD belongs to a package',
    ko: '이 MOD는 패키지에 속합니다',
    ja: 'このMODはパッケージに属します',
  },
  // 它与 {count} 个子 MOD 同属一个包，一并删除？
  itSharesAPackageWithModsDeleteThemTogether: {
    'zh-CN': '它与 {count} 个子 MOD 同属一个包，一并删除？',
    'zh-TW': '它與 {count} 個子 MOD 同屬一個包，一併刪除？',
    en: 'It shares a package with {count} MODs. Delete them together?',
    ko: '{count}개 MOD와 같은 패키지입니다. 함께 삭제할까요?',
    ja: '{count} 件のMODと同じパッケージです。まとめて削除しますか？',
  },
  // 整包删除
  deletePackage: {
    'zh-CN': '整包删除',
    'zh-TW': '整包刪除',
    en: 'Delete package',
    ko: '패키지 삭제',
    ja: 'パッケージを削除',
  },
  // 切换布局（卡片 / 横排）
  switchLayoutCardsRows: {
    'zh-CN': '切换布局（卡片 / 横排）',
    'zh-TW': '切換佈局（卡片 / 橫排）',
    en: 'Switch layout (cards / rows)',
    ko: '레이아웃 전환(카드/행)',
    ja: 'レイアウト切替（カード／行）',
  },
  // 排序：{sort}
  sort: {
    'zh-CN': '排序：{sort}',
    'zh-TW': '排序：{sort}',
    en: 'Sort: {sort}',
    ko: '정렬: {sort}',
    ja: '並び: {sort}',
  },
  // 全部刷新（逐仓独立，互不阻塞）
  refreshAllPerRepositoryNonBlocking: {
    'zh-CN': '全部刷新（逐仓独立，互不阻塞）',
    'zh-TW': '全部重新整理（逐倉獨立，互不阻塞）',
    en: 'Refresh all (per repository, non-blocking)',
    ko: '모두 새로 고침(저장소별 독립)',
    ja: 'すべて更新（リポジトリ単位・非ブロッキング）',
  },
  // 打开所在文件夹并定位
  revealInFolder: {
    'zh-CN': '打开所在文件夹并定位',
    'zh-TW': '開啟所在資料夾並定位',
    en: 'Reveal in folder',
    ko: '폴더에서 위치 열기',
    ja: 'フォルダーで表示',
  },
  // 查看日志
  viewLogs: {
    'zh-CN': '查看日志',
    'zh-TW': '檢視日誌',
    en: 'View logs',
    ko: '로그 보기',
    ja: 'ログを表示',
  },
  // 打开目录
  openFolder: {
    'zh-CN': '打开目录',
    'zh-TW': '開啟目錄',
    en: 'Open folder',
    ko: '폴더 열기',
    ja: 'フォルダーを開く',
  },
  // 打开
  open: {
    'zh-CN': '打开',
    'zh-TW': '開啟',
    en: 'Open',
    ko: '열기',
    ja: '開く',
  },
  // 标题
  title: {
    'zh-CN': '标题',
    'zh-TW': '標題',
    en: 'Title',
    ko: '제목',
    ja: 'タイトル',
  },
  // 产品名
  productName: {
    'zh-CN': '产品名',
    'zh-TW': '產品名',
    en: 'Product name',
    ko: '제품명',
    ja: '製品名',
  },
  // 原始文件名
  originalFilename: {
    'zh-CN': '原始文件名',
    'zh-TW': '原始檔名',
    en: 'Original filename',
    ko: '원본 파일명',
    ja: '元のファイル名',
  },
  // 文件
  file2: {
    'zh-CN': '文件',
    'zh-TW': '檔案',
    en: 'File',
    ko: '파일',
    ja: 'ファイル',
  },
  // 插件名
  pluginName: {
    'zh-CN': '插件名',
    'zh-TW': '外掛名',
    en: 'Plugin name',
    ko: '플러그인 이름',
    ja: 'プラグイン名',
  },
  // 作者
  author: {
    'zh-CN': '作者',
    'zh-TW': '作者',
    en: 'Author',
    ko: '제작자',
    ja: '作者',
  },
  // 版权
  copyright: {
    'zh-CN': '版权',
    'zh-TW': '版權',
    en: 'Copyright',
    ko: '저작권',
    ja: '著作権',
  },
  // 版本来源
  versionSource: {
    'zh-CN': '版本来源',
    'zh-TW': '版本來源',
    en: 'Version source',
    ko: '버전 출처',
    ja: 'バージョン取得元',
  },
  // 启用状态
  enabled: {
    'zh-CN': '启用状态',
    'zh-TW': '啟用狀態',
    en: 'Enabled',
    ko: '활성 상태',
    ja: '有効状態',
  },
  // 配置文件
  configFile: {
    'zh-CN': '配置文件',
    'zh-TW': '設定檔',
    en: 'Config file',
    ko: '설정 파일',
    ja: '設定ファイル',
  },
  // 重复 GUID
  duplicateGuid: {
    'zh-CN': '重复 GUID',
    'zh-TW': '重複 GUID',
    en: 'Duplicate GUID',
    ko: '중복 GUID',
    ja: '重複 GUID',
  },
  // 已启用
  enabled2: {
    'zh-CN': '已启用',
    'zh-TW': '已啟用',
    en: 'Enabled',
    ko: '활성',
    ja: '有効',
  },
  // 已安装
  installed: {
    'zh-CN': '已安装',
    'zh-TW': '已安裝',
    en: 'Installed',
    ko: '설치됨',
    ja: 'インストール済み',
  },
  // 有
  yes: {
    'zh-CN': '有',
    'zh-TW': '有',
    en: 'Yes',
    ko: '있음',
    ja: 'あり',
  },
  // 无
  no: {
    'zh-CN': '无',
    'zh-TW': '無',
    en: 'No',
    ko: '없음',
    ja: 'なし',
  },
  // 是
  yes2: {
    'zh-CN': '是',
    'zh-TW': '是',
    en: 'Yes',
    ko: '예',
    ja: 'はい',
  },
  // 否
  no2: {
    'zh-CN': '否',
    'zh-TW': '否',
    en: 'No',
    ko: '아니요',
    ja: 'いいえ',
  },
  // 状态标签
  statusLabels: {
    'zh-CN': '状态标签',
    'zh-TW': '狀態標籤',
    en: 'Status labels',
    ko: '상태 라벨',
    ja: 'ステータスラベル',
  },
  // 提交日志
  releaseNotes: {
    'zh-CN': '提交日志',
    'zh-TW': '提交日誌',
    en: 'Release notes',
    ko: '릴리스 노트',
    ja: 'リリースノート',
  },
  // 预发布版本
  preReleaseVersion: {
    'zh-CN': '预发布版本',
    'zh-TW': '預發布版本',
    en: 'Pre-release version',
    ko: '프리릴리스 버전',
    ja: 'プレリリース版',
  },
  // 此版本为预览版本，可能存在未知问题，是否继续安装？
  thisIsAPreReleaseBuildAndMayHaveUnknownIssuesContinueInstalling: {
    'zh-CN': '此版本为预览版本，可能存在未知问题，是否继续安装？',
    'zh-TW': '此版本為預覽版本，可能存在未知問題，是否繼續安裝？',
    en: 'This is a pre-release build and may have unknown issues. Continue installing?',
    ko: '이 버전은 프리릴리스이며 알려지지 않은 문제가 있을 수 있습니다. 계속 설치할까요?',
    ja: 'このバージョンはプレリリースのため、未知の問題が含まれる可能性があります。インストールを続行しますか？',
  },
  // 继续安装
  continueInstalling: {
    'zh-CN': '继续安装',
    'zh-TW': '繼續安裝',
    en: 'Continue installing',
    ko: '계속 설치',
    ja: 'インストールを続行',
  },
  // 该版本没有提交日志
  thisReleaseHasNoNotes: {
    'zh-CN': '该版本没有提交日志',
    'zh-TW': '該版本沒有提交日誌',
    en: 'This release has no notes',
    ko: '이 버전에는 릴리스 노트가 없습니다',
    ja: 'このバージョンにはリリースノートがありません',
  },
  // 绑定仓库
  boundRepository: {
    'zh-CN': '绑定仓库',
    'zh-TW': '綁定倉庫',
    en: 'Bound repository',
    ko: '연결된 저장소',
    ja: '紐付けリポジトリ',
  },
  // 删除
  delete: {
    'zh-CN': '删除',
    'zh-TW': '刪除',
    en: 'Delete',
    ko: '삭제',
    ja: '削除',
  },
  // 已移入回收站
  movedToRecycleBin: {
    'zh-CN': '已移入回收站',
    'zh-TW': '已移入資源回收筒',
    en: 'Moved to the recycle bin',
    ko: '휴지통으로 옮겼습니다',
    ja: 'ゴミ箱へ移動しました',
  },
  // 已删除
  deleted: {
    'zh-CN': '已删除',
    'zh-TW': '已刪除',
    en: 'Deleted',
    ko: '삭제되었습니다',
    ja: '削除しました',
  },
}
