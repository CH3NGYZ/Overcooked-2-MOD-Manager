import type { DomainMessages } from '../types.ts'

/** 仓库详情、版本选择、刷新状态与下载进度。同一 key 的五语保持集中。 */
export const toolsUiRepository: DomainMessages = {
  mergingVolumes: { 'zh-CN': '正在重组并校验分卷', 'zh-TW': '正在重組並校驗分卷', en: 'Reassembling and verifying volumes', ko: '분할 파일 병합 및 검증 중', ja: '分割ファイルの再構築と検証中' },
  volumeSummary: { 'zh-CN': '分卷：已完成 {done}/{total}', 'zh-TW': '分卷：已完成 {done}/{total}', en: 'Volumes: {done}/{total} completed', ko: '분할 파일: {done}/{total} 완료', ja: '分割ファイル：{done}/{total} 完了' },
  volumeRetry: { 'zh-CN': '整卷重试 {count}/3', 'zh-TW': '整卷重試 {count}/3', en: 'Volume retries {count}/3', ko: '분할 파일 재시도 {count}/3', ja: '分割ファイル再試行 {count}/3' },
  'volumeState.pending': { 'zh-CN': '排队中', 'zh-TW': '排隊中', en: 'Queued', ko: '대기 중', ja: '待機中' },
  'volumeState.downloading': { 'zh-CN': '下载中', 'zh-TW': '下載中', en: 'Downloading', ko: '다운로드 중', ja: 'ダウンロード中' },
  'volumeState.completed': { 'zh-CN': '已完成', 'zh-TW': '已完成', en: 'Completed', ko: '완료', ja: '完了' },
  'volumeState.failed': { 'zh-CN': '失败', 'zh-TW': '失敗', en: 'Failed', ko: '실패', ja: '失敗' },
  // 说明
  details: {
    'zh-CN': '说明',
    'zh-TW': '說明',
    en: 'Details',
    ko: '설명',
    ja: '説明',
  },
  // 下载安装
  download: {
    'zh-CN': '下载安装',
    'zh-TW': '下載安裝',
    en: 'Download',
    ko: '다운로드 및 설치',
    ja: 'ダウンロードとインストール',
  },
  // 先刷新源文档，再并行刷新各仓库
  reloadSourceDocumentsThenRefreshRepositoriesInParallel: {
    'zh-CN': '先刷新源文档，再并行刷新各仓库',
    'zh-TW': '先重新整理來源文件，再並行整理各倉庫',
    en: 'Reload source documents, then refresh repositories in parallel',
    ko: '소스 문서를 먼저 새로 고친 뒤 저장소를 병렬로 새로 고침',
    ja: 'ソース文書を更新してから各リポジトリを並列更新',
  },
  // 仓库源刷新失败，请稍后重试
  sourceRefreshFailedTryAgainLater: {
    'zh-CN': '仓库源刷新失败，请稍后重试',
    'zh-TW': '倉庫源重新整理失敗，請稍後重試',
    en: 'Source refresh failed. Try again later.',
    ko: '소스 새로 고침 실패, 잠시 후 다시 시도하세요',
    ja: 'ソースの更新に失敗しました。後で再試行してください',
  },
  // 许可证
  license: {
    'zh-CN': '许可证',
    'zh-TW': '授權條款',
    en: 'License',
    ko: '라이선스',
    ja: 'ライセンス',
  },
  // 最后更新
  updated: {
    'zh-CN': '最后更新',
    'zh-TW': '最後更新',
    en: 'Updated',
    ko: '최종 업데이트',
    ja: '最終更新',
  },
  // 标识
  identifier: {
    'zh-CN': '标识',
    'zh-TW': '識別碼',
    en: 'Identifier',
    ko: '식별자',
    ja: '識別子',
  },
  // 版本绑定
  versionBinding: {
    'zh-CN': '版本绑定',
    'zh-TW': '版本綁定',
    en: 'Version binding',
    ko: '버전 바인딩',
    ja: 'バージョン紐付け',
  },
  // 源码
  source: {
    'zh-CN': '源码',
    'zh-TW': '原始碼',
    en: 'Source',
    ko: '소스',
    ja: 'ソース',
  },
  // Release
  release: {
    'zh-CN': 'Release',
    'zh-TW': 'Release',
    en: 'Release',
    ko: 'Release',
    ja: 'Release',
  },
  // 该仓库没有备注
  thisRepositoryHasNoNote: {
    'zh-CN': '该仓库没有备注',
    'zh-TW': '該倉庫沒有備註',
    en: 'This repository has no note',
    ko: '이 저장소에는 메모가 없습니다',
    ja: 'このリポジトリにはメモがありません',
  },
  // 正在加载仓库说明
  loadingRepositoryReadme: {
    'zh-CN': '正在加载仓库说明',
    'zh-TW': '正在載入倉庫說明',
    en: 'Loading repository README',
    ko: '저장소 README 불러오는 중',
    ja: 'リポジトリの説明を読み込み中',
  },
  // 没有匹配的仓库
  noMatchingRepositories: {
    'zh-CN': '没有匹配的仓库',
    'zh-TW': '沒有符合的倉庫',
    en: 'No matching repositories',
    ko: '일치하는 저장소가 없습니다',
    ja: '一致するリポジトリがありません',
  },
  // 部分启用
  partiallyEnabled: {
    'zh-CN': '部分启用',
    'zh-TW': '部分啟用',
    en: 'Partially enabled',
    ko: '일부 활성',
    ja: '一部有効',
  },
  // 可更新
  update: {
    'zh-CN': '可更新',
    'zh-TW': '可更新',
    en: 'Update',
    ko: '업데이트 가능',
    ja: '更新あり',
  },
  // 刷新失败
  refreshFailed: {
    'zh-CN': '刷新失败',
    'zh-TW': '重新整理失敗',
    en: 'Refresh failed',
    ko: '새로 고침 실패',
    ja: '更新失敗',
  },
  // 已最新
  upToDate: {
    'zh-CN': '已最新',
    'zh-TW': '已最新',
    en: 'Up to date',
    ko: '최신 상태',
    ja: '最新です',
  },
  // 低版本
  olderVersion: {
    'zh-CN': '低版本',
    'zh-TW': '低版本',
    en: 'Older version',
    ko: '이전 버전',
    ja: '旧バージョン',
  },
  // 版本未知
  versionUnknown: {
    'zh-CN': '版本未知',
    'zh-TW': '版本未知',
    en: 'Version unknown',
    ko: '버전 미확인',
    ja: 'バージョン不明',
  },
  // 检测中
  checking: {
    'zh-CN': '检测中',
    'zh-TW': '檢測中',
    en: 'Checking',
    ko: '확인 중',
    ja: '確認中',
  },
  // 刷新中（绑定仓库 / 地图源的 Release 正在重抓）
  refreshing: {
    'zh-CN': '刷新中',
    'zh-TW': '重新整理中',
    en: 'Refreshing',
    ko: '새로 고침 중',
    ja: '更新中',
  },
  // 正在检测更新
  checkingForUpdates: {
    'zh-CN': '正在检测更新',
    'zh-TW': '正在檢查更新',
    en: 'Checking for updates',
    ko: '업데이트 확인 중',
    ja: '更新を確認中',
  },
  // 排序
  sort: {
    'zh-CN': '排序',
    'zh-TW': '排序',
    en: 'Sort',
    ko: '정렬',
    ja: '並び替え',
  },
  // 重新扫描本地 MOD
  rescanLocalMods: {
    'zh-CN': '重新扫描本地 MOD',
    'zh-TW': '重新掃描本機 MOD',
    en: 'Rescan local MODs',
    ko: '로컬 MOD 다시 검사',
    ja: 'ローカル MOD を再スキャン',
  },
  // 已重新扫描本地 MOD
  localModsRescanned: {
    'zh-CN': '已重新扫描本地 MOD',
    'zh-TW': '已重新掃描本機 MOD',
    en: 'Local MODs rescanned',
    ko: '로컬 MOD를 다시 검사했습니다',
    ja: 'ローカル MOD を再スキャンしました',
  },
  // 准备中
  preparing: {
    'zh-CN': '准备中',
    'zh-TW': '準備中',
    en: 'Preparing',
    ko: '준비 중',
    ja: '準備中',
  },
  // 准备中…
  preparing2: {
    'zh-CN': '准备中…',
    'zh-TW': '準備中…',
    en: 'Preparing…',
    ko: '준비 중…',
    ja: '準備中…',
  },
  // 下载失败
  downloadFailed: {
    'zh-CN': '下载失败',
    'zh-TW': '下載失敗',
    en: 'Download failed',
    ko: '다운로드 실패',
    ja: 'ダウンロード失敗',
  },
  // 正在连接下载源…
  connectingToDownloadSource: {
    'zh-CN': '正在连接下载源…',
    'zh-TW': '正在連線下載來源…',
    en: 'Connecting to download source…',
    ko: '다운로드 소스에 연결 중…',
    ja: 'ダウンロード元に接続中…',
  },
  // 正在切换更快的源…
  switchingToAFasterSource: {
    'zh-CN': '正在切换更快的源…',
    'zh-TW': '正在切換更快的來源…',
    en: 'Switching to a faster source…',
    ko: '더 빠른 소스로 전환 중…',
    ja: 'より速いソースに切り替え中…',
  },
  // 正在下载
  downloading: {
    'zh-CN': '正在下载',
    'zh-TW': '正在下載',
    en: 'Downloading',
    ko: '다운로드 중',
    ja: 'ダウンロード中',
  },
  // 已完成
  completed: {
    'zh-CN': '已完成',
    'zh-TW': '已完成',
    en: 'Completed',
    ko: '완료',
    ja: '完了',
  },
  // {count} 源竞速
  sourcesRacing: {
    'zh-CN': '{count} 源竞速',
    'zh-TW': '{count} 源競速',
    en: '{count} sources racing',
    ko: '{count}개 소스 경쟁',
    ja: '{count} ソース競争',
  },
  // 第 {current}/{total} 次尝试
  attempt: {
    'zh-CN': '第 {current}/{total} 次尝试',
    'zh-TW': '第 {current}/{total} 次嘗試',
    en: 'Attempt {current}/{total}',
    ko: '{current}/{total}번째 시도',
    ja: '{current}/{total} 回目の試行',
  },
  // 选择版本
  selectVersion: {
    'zh-CN': '选择版本',
    'zh-TW': '選擇版本',
    en: 'Select version',
    ko: '버전 선택',
    ja: 'バージョンを選択',
  },
  // 选择文件
  selectFile: {
    'zh-CN': '选择文件',
    'zh-TW': '選擇檔案',
    en: 'Select file',
    ko: '파일 선택',
    ja: 'ファイルを選択',
  },
  // 大小
  size: {
    'zh-CN': '大小',
    'zh-TW': '大小',
    en: 'Size',
    ko: '크기',
    ja: 'サイズ',
  },
  // 下载 {count} 次
  downloads: {
    'zh-CN': '下载 {count} 次',
    'zh-TW': '下載 {count} 次',
    en: '{count} downloads',
    ko: '{count}회 다운로드',
    ja: '{count} 回ダウンロード',
  },
  // 另有 {count} 个匹配文件
  moreMatchingFiles: {
    'zh-CN': '另有 {count} 个匹配文件',
    'zh-TW': '另有 {count} 個符合檔案',
    en: '{count} more matching files',
    ko: '{count}개 추가 일치 파일',
    ja: '他に {count} 件の一致ファイル',
  },
  // 该版本没有命中资产规则的文件
  noAssetInThisReleaseMatchesTheAssetRules: {
    'zh-CN': '该版本没有命中资产规则的文件',
    'zh-TW': '該版本沒有符合資產規則的檔案',
    en: 'No asset in this release matches the asset rules',
    ko: '이 버전에는 자산 규칙과 일치하는 파일이 없습니다',
    ja: 'このバージョンにアセット規則と一致するファイルがありません',
  },
  // 仓库文件模式直接下载该文件
  repositoryFileModeDownloadsThisFileDirectly: {
    'zh-CN': '仓库文件模式直接下载该文件',
    'zh-TW': '倉庫檔案模式直接下載該檔案',
    en: 'ModRepository-file mode downloads this file directly',
    ko: '저장소 파일 모드는 이 파일을 바로 받습니다',
    ja: 'リポジトリファイルモードはこのファイルを直接ダウンロードします',
  },
  // 当前渠道没有社区仓库
  noCommunityRepositoriesInThisChannel: {
    'zh-CN': '当前渠道没有社区仓库',
    'zh-TW': '目前渠道沒有社群倉庫',
    en: 'No community repositories in this channel',
    ko: '이 채널에는 커뮤니티 저장소가 없습니다',
    ja: 'このチャンネルにはコミュニティリポジトリがありません',
  },
}
