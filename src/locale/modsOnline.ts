import type { DomainMessages } from './types'

/**
 * modsOnline 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const modsOnline: DomainMessages = {
  // 共 {count} 个仓库
  repositories: {
    'zh-CN': '共 {count} 个仓库',
    'zh-TW': '共 {count} 個倉庫',
    en: '{count} repositories',
    ko: '저장소 {count}개',
    ja: 'リポジトリ {count} 件',
  },
  // 正在读取仓库源（整页加载遮罩文案）
  readingRepositorySources: {
    'zh-CN': '正在读取仓库源…',
    'zh-TW': '正在讀取倉庫來源…',
    en: 'Reading repository sources…',
    ko: '저장소 소스를 읽는 중…',
    ja: 'リポジトリソースを読み込み中…',
  },
  // 全部刷新
  refreshAll: {
    'zh-CN': '全部刷新',
    'zh-TW': '全部重新整理',
    en: 'Refresh all',
    ko: '모두 새로 고침',
    ja: 'すべて更新',
  },
  // 暂无仓库配置
  noRepositoriesConfigured: {
    'zh-CN': '暂无仓库配置',
    'zh-TW': '暫無倉庫設定',
    en: 'No repositories configured',
    ko: '저장소 설정 없음',
    ja: 'リポジトリ設定がありません',
  },
  // 仓库文件
  repositoryFile: {
    'zh-CN': '仓库文件',
    'zh-TW': '倉庫檔案',
    en: 'ModRepository file',
    ko: '저장소 파일',
    ja: 'リポジトリファイル',
  },
  // 未启用
  disabled: {
    'zh-CN': '未启用',
    'zh-TW': '未啟用',
    en: 'Disabled',
    ko: '비활성',
    ja: '無効',
  },
  // 仓库刷新失败
  repositoryRefreshFailed: {
    'zh-CN': '仓库刷新失败',
    'zh-TW': '倉庫重新整理失敗',
    en: 'ModRepository refresh failed',
    ko: '저장소 새로 고침 실패',
    ja: 'リポジトリ更新失敗',
  },
  // 刷新
  refresh: {
    'zh-CN': '刷新',
    'zh-TW': '重新整理',
    en: 'Refresh',
    ko: '새로 고침',
    ja: '更新',
  },
  // 安装
  install: {
    'zh-CN': '安装',
    'zh-TW': '安裝',
    en: 'Install',
    ko: '설치',
    ja: 'インストール',
  },
  // 版本
  versions: {
    'zh-CN': '版本',
    'zh-TW': '版本',
    en: 'Versions',
    ko: '버전',
    ja: 'バージョン',
  },
  // 未刷新
  notRefreshed: {
    'zh-CN': '未刷新',
    'zh-TW': '未重新整理',
    en: 'Not refreshed',
    ko: '새로 고침 안 됨',
    ja: '未更新',
  },
  // {count} 个版本
  versions2: {
    'zh-CN': '{count} 个版本',
    'zh-TW': '{count} 個版本',
    en: '{count} versions',
    ko: '{count}개 버전',
    ja: '{count} バージョン',
  },
  // 预发布
  preRelease: {
    'zh-CN': '预发布',
    'zh-TW': '預發布',
    en: 'Pre-release',
    ko: '프리릴리스',
    ja: 'プレリリース',
  },
  // 安装依赖
  installDependency: {
    'zh-CN': '安装依赖',
    'zh-TW': '安裝依賴',
    en: 'Install dependency',
    ko: '의존성 설치',
    ja: '依存関係をインストール',
  },
  // 缺少依赖
  missingDependency: {
    'zh-CN': '缺少依赖',
    'zh-TW': '缺少依賴',
    en: 'Missing dependency',
    ko: '의존성 누락',
    ja: '依存関係が不足',
  },
  // 缺依赖
  missingDep: {
    'zh-CN': '缺依赖',
    'zh-TW': '缺依賴',
    en: 'Missing dep',
    ko: '의존성 없음',
    ja: '依存不足',
  },
  // 有冲突（被动描述：卡片自己装上了，但和别的插件撞了，唯一出路是卸载）
  conflict: {
    'zh-CN': '有冲突',
    'zh-TW': '有衝突',
    en: 'Has conflict',
    ko: '충돌 있음',
    ja: '競合あり',
  },
  // 去卸载
  uninstall: {
    'zh-CN': '去卸载',
    'zh-TW': '去解除安裝',
    en: 'Uninstall',
    ko: '제거하기',
    ja: 'アンインストールへ',
  },
  // 卸载冲突的 MOD
  uninstallConflictingMod: {
    'zh-CN': '卸载冲突的 MOD',
    'zh-TW': '解除安裝衝突的 MOD',
    en: 'Uninstall conflicting MOD',
    ko: '충돌 MOD 제거',
    ja: '競合 MOD をアンインストール',
  },
  // 「{owner}」与「{target}」不兼容，需要卸载
  isIncompatibleWithAndMustBeUninstalled: {
    'zh-CN': '「{owner}」与「{target}」不兼容，需要卸载',
    'zh-TW': '「{owner}」與「{target}」不相容，需要解除安裝',
    en: '"{owner}" is incompatible with "{target}" and must be uninstalled',
    ko: '"{owner}"은(는) "{target}"와(과) 호환되지 않아 제거가 필요합니다',
    ja: '「{owner}」は「{target}」と互換性がないため、アンインストールが必要です',
  },
  // 不兼容
  incompatible: {
    'zh-CN': '不兼容',
    'zh-TW': '不相容',
    en: 'Incompatible',
    ko: '비호환',
    ja: '非互換',
  },
  // 「{name}」与「{owner}」不兼容，需要卸载。继续？
  isIncompatibleWithAndMustBeUninstalledContinue: {
    'zh-CN': '「{name}」与「{owner}」不兼容，需要卸载。继续？',
    'zh-TW': '「{name}」與「{owner}」不相容，需要解除安裝。繼續？',
    en: '"{name}" is incompatible with "{owner}" and must be uninstalled. Continue?',
    ko: '"{name}"은(는) "{owner}"와(과) 호환되지 않아 제거가 필요합니다. 계속할까요?',
    ja: '「{name}」は「{owner}」と互換性がないため、アンインストールが必要です。続行しますか？',
  },
  // 仅使用内置仓库源
  useBundledSourcesOnly: {
    'zh-CN': '仅使用内置仓库源',
    'zh-TW': '僅使用內建倉庫來源',
    en: 'Use bundled sources only',
    ko: '내장 저장소 소스만 사용',
    ja: '組み込みソースのみ使用',
  },
  // 不访问 Gitee / GitHub 的仓库源直链，直接使用管理器自带的快照；自定义仓库源不受影响
  skipsTheGiteeGithubRawSourceLinksAndUsesTheSnapshotShippedWithTheManagerCustomSourcesAreNotAffected: {
    'zh-CN': '不访问 Gitee / GitHub 的仓库源直链，直接使用管理器自带的快照；自定义仓库源不受影响',
    'zh-TW': '不存取 Gitee / GitHub 的倉庫來源直鏈，直接使用管理器自帶的快照；自訂倉庫來源不受影響',
    en: 'Skips the Gitee / GitHub raw source links and uses the snapshot shipped with the manager. Custom sources are not affected.',
    ko: 'Gitee / GitHub raw 링크를 사용하지 않고 관리자에 포함된 스냅샷을 사용합니다. 사용자 지정 소스는 영향받지 않습니다.',
    ja: 'Gitee / GitHub の raw リンクを使わず、マネージャー同梱のスナップショットを使用します。カスタムソースには影響しません。',
  },
  // 已切换到内置仓库源，刷新后生效
  switchedToBundledSourcesRefreshToApply: {
    'zh-CN': '已切换到内置仓库源，刷新后生效',
    'zh-TW': '已切換到內建倉庫來源，重新整理後生效',
    en: 'Switched to bundled sources. Refresh to apply.',
    ko: '내장 저장소 소스로 전환했습니다. 새로 고침 후 적용됩니다.',
    ja: '組み込みソースに切り替えました。更新後に反映されます。',
  },
  // 已恢复在线仓库源，刷新后生效
  restoredOnlineSourcesRefreshToApply: {
    'zh-CN': '已恢复在线仓库源，刷新后生效',
    'zh-TW': '已恢復線上倉庫來源，重新整理後生效',
    en: 'Restored online sources. Refresh to apply.',
    ko: '온라인 저장소 소스로 복원했습니다. 새로 고침 후 적용됩니다.',
    ja: 'オンラインソースに戻しました。更新後に反映されます。',
  },
  // 安装前检查
  preInstallCheck: {
    'zh-CN': '安装前检查',
    'zh-TW': '安裝前檢查',
    en: 'Pre-install check',
    ko: '설치 전 검사',
    ja: 'インストール前の確認',
  },
  // 安装「{name}」前，需要先处理以下关系：
  beforeInstallingTheseRelationsMustBeHandledFirst: {
    'zh-CN': '安装「{name}」前，需要先处理以下关系：',
    'zh-TW': '安裝「{name}」前，需要先處理以下關係：',
    en: 'Before installing "{name}", these relations must be handled first:',
    ko: '"{name}" 설치 전에 다음 관계를 먼저 처리해야 합니다:',
    ja: '「{name}」をインストールする前に、以下の関係を処理する必要があります：',
  },
  // 全部处理
  handleAll: {
    'zh-CN': '全部处理',
    'zh-TW': '全部處理',
    en: 'Handle all',
    ko: '모두 처리',
    ja: 'すべて処理',
  },
  // 仍然安装
  installAnyway: {
    'zh-CN': '仍然安装',
    'zh-TW': '仍然安裝',
    en: 'Install anyway',
    ko: '그래도 설치',
    ja: 'それでもインストール',
  },
  // 完成
  done: {
    'zh-CN': '完成',
    'zh-TW': '完成',
    en: 'Done',
    ko: '완료',
    ja: '完了',
  },
  // 下载完成但没有解析出安装内容
  downloadFinishedButNoInstallableContentWasParsed: {
    'zh-CN': '下载完成但没有解析出安装内容',
    'zh-TW': '下載完成但沒有解析出安裝內容',
    en: 'Download finished but no installable content was parsed',
    ko: '다운로드는 완료되었지만 설치할 내용을 분석하지 못했습니다',
    ja: 'ダウンロードは完了しましたが、インストール内容を解析できませんでした',
  },
  // 正在读取配置
  readingConfiguration: {
    'zh-CN': '正在读取配置',
    'zh-TW': '正在讀取設定',
    en: 'Reading configuration',
    ko: '설정 읽는 중',
    ja: '設定を読み込み中',
  },
  // 保存
  save: {
    'zh-CN': '保存',
    'zh-TW': '儲存',
    en: 'Save',
    ko: '저장',
    ja: '保存',
  },
  // 已复制
  copied: {
    'zh-CN': '已复制',
    'zh-TW': '已複製',
    en: 'Copied',
    ko: '복사되었습니다',
    ja: 'コピーしました',
  },
  // 复制
  copy: {
    'zh-CN': '复制',
    'zh-TW': '複製',
    en: 'Copy',
    ko: '복사',
    ja: 'コピー',
  },
  // 整包移入回收站
  movePackageToRecycleBin: {
    'zh-CN': '整包移入回收站',
    'zh-TW': '整包移入資源回收筒',
    en: 'Move package to recycle bin',
    ko: '패키지째 휴지통으로 이동',
    ja: 'パッケージをゴミ箱へ移動',
  },
  // 已完成 {count} 项处理
  handledItemS: {
    'zh-CN': '已完成 {count} 项处理',
    'zh-TW': '已完成 {count} 項處理',
    en: 'Handled {count} item(s)',
    ko: '{count}개 항목을 처리했습니다',
    ja: '{count} 件を処理しました',
  },
  // 找不到要卸载的 MOD
  theModToUninstallWasNotFound: {
    'zh-CN': '找不到要卸载的 MOD',
    'zh-TW': '找不到要解除安裝的 MOD',
    en: 'The MOD to uninstall was not found',
    ko: '제거할 MOD를 찾을 수 없습니다',
    ja: 'アンインストール対象の MOD が見つかりません',
  },
  // 找不到可安装的来源
  noInstallableSourceFound: {
    'zh-CN': '找不到可安装的来源',
    'zh-TW': '找不到可安裝的來源',
    en: 'No installable source found',
    ko: '설치 가능한 소스를 찾을 수 없습니다',
    ja: 'インストール可能なソースが見つかりません',
  },
  // 「{name}」{version} 不兼容，需要 {range} 之间的版本
  isIncompatibleAVersionInIsRequired: {
    'zh-CN': '「{name}」{version} 不兼容，需要 {range} 之间的版本',
    'zh-TW': '「{name}」{version} 不相容，需要 {range} 之間的版本',
    en: '"{name}" {version} is incompatible; a version in {range} is required',
    ko: '"{name}" {version}은(는) 호환되지 않습니다. {range} 버전이 필요합니다',
    ja: '「{name}」{version} は非互換です。{range} のバージョンが必要です',
  },
  // 「{name}」{version} 不兼容，请注意
  isIncompatiblePleaseCheck: {
    'zh-CN': '「{name}」{version} 不兼容，请注意',
    'zh-TW': '「{name}」{version} 不相容，請注意',
    en: '"{name}" {version} is incompatible; please check',
    ko: '"{name}" {version}은(는) 호환되지 않습니다. 확인해 주세요',
    ja: '「{name}」{version} は非互換です。ご確認ください',
  },
  // 「{name}」{version} 冲突，请卸载
  conflictsPleaseUninstallIt: {
    'zh-CN': '「{name}」{version} 冲突，请卸载',
    'zh-TW': '「{name}」{version} 衝突，請解除安裝',
    en: '"{name}" {version} conflicts; please uninstall it',
    ko: '"{name}" {version}이(가) 충돌합니다. 제거해 주세요',
    ja: '「{name}」{version} が競合しています。アンインストールしてください',
  },
  // 缺少依赖「{name}」，需要 {range} 之间的版本
  missingDependencyAVersionInIsRequired: {
    'zh-CN': '缺少依赖「{name}」，需要 {range} 之间的版本',
    'zh-TW': '缺少依賴「{name}」，需要 {range} 之間的版本',
    en: 'Missing dependency "{name}"; a version in {range} is required',
    ko: '의존성 "{name}"이(가) 없습니다. {range} 버전이 필요합니다',
    ja: '依存関係「{name}」がありません。{range} のバージョンが必要です',
  },
  // 缺少依赖「{name}」
  missingDependency2: {
    'zh-CN': '缺少依赖「{name}」',
    'zh-TW': '缺少依賴「{name}」',
    en: 'Missing dependency "{name}"',
    ko: '의존성 "{name}"이(가) 없습니다',
    ja: '依存関係「{name}」がありません',
  },
  // 测
  t: {
    'zh-CN': '测',
    'zh-TW': '測',
    en: 'T',
    ko: '테',
    ja: '試',
  },
  // 安装此版本
  installThisVersion: {
    'zh-CN': '安装此版本',
    'zh-TW': '安裝此版本',
    en: 'Install this version',
    ko: '이 버전 설치',
    ja: 'このバージョンをインストール',
  },
  // 尚无缓存的 Release，请先刷新。
  noCachedReleasesRefreshFirst: {
    'zh-CN': '尚无缓存的 Release，请先刷新。',
    'zh-TW': '尚無快取的 Release，請先重新整理。',
    en: 'No cached releases. Refresh first.',
    ko: '캐시된 Release가 없습니다. 먼저 새로 고침하세요.',
    ja: 'キャッシュされたReleaseがありません。先に更新してください。',
  },
  // 游戏运行中，已禁止安装。请先关闭游戏。
  theGameIsRunningCloseItBeforeInstalling: {
    'zh-CN': '游戏运行中，已禁止安装。请先关闭游戏。',
    'zh-TW': '遊戲執行中，已禁止安裝。請先關閉遊戲。',
    en: 'The game is running. Close it before installing.',
    ko: '게임 실행 중에는 설치할 수 없습니다. 먼저 게임을 종료하세요.',
    ja: 'ゲーム実行中はインストールできません。先にゲームを終了してください。',
  },
  // 获取到 {count} 个 Release
  fetchedReleases: {
    'zh-CN': '获取到 {count} 个 Release',
    'zh-TW': '取得 {count} 個 Release',
    en: 'Fetched {count} releases',
    ko: 'Release {count}개를 가져왔습니다',
    ja: 'Release を {count} 件取得しました',
  },
  // 在 MOD 安装中查看依赖
  viewDependencyInModInstall: {
    'zh-CN': '在 MOD 安装中查看依赖',
    'zh-TW': '在 MOD 安裝中檢視依賴',
    en: 'View dependency in MOD Install',
    ko: 'MOD 설치에서 의존성 보기',
    ja: 'MOD インストールで依存関係を表示',
  },
  // 没有可安装的来源
  noInstallableSource: {
    'zh-CN': '没有可安装的来源',
    'zh-TW': '沒有可安裝的來源',
    en: 'No installable source',
    ko: '설치 가능한 소스 없음',
    ja: 'インストール可能なソースがありません',
  },
  // 点击定位到该仓库
  clickToLocateThatRepository: {
    'zh-CN': '点击定位到该仓库',
    'zh-TW': '點擊定位到該倉庫',
    en: 'Click to locate that repository',
    ko: '클릭하면 해당 저장소로 이동합니다',
    ja: 'クリックでそのリポジトリへ移動します',
  },
  // 在 MOD 管理中查看
  viewInModManager: {
    'zh-CN': '在 MOD 管理中查看',
    'zh-TW': '在 MOD 管理中檢視',
    en: 'View in MOD Manager',
    ko: 'MOD 관리에서 보기',
    ja: 'MOD マネージャーで表示',
  },
  // 在页面内定位
  locateOnThisPage: {
    'zh-CN': '在页面内定位',
    'zh-TW': '在頁面內定位',
    en: 'Locate on this page',
    ko: '페이지에서 찾기',
    ja: 'このページ内で表示',
  },
  // {provider} 社区仓库：{reason}
  communityRepositoryError: {
    'zh-CN': '{provider} 社区仓库：{reason}',
    'zh-TW': '{provider} 社群倉庫：{reason}',
    en: '{provider} community repositories: {reason}',
    ko: '{provider} 커뮤니티 저장소: {reason}',
    ja: '{provider} コミュニティリポジトリ: {reason}',
  },
}
