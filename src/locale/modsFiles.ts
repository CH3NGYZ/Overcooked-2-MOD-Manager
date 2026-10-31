import type { DomainMessages } from './types'

/**
 * MOD 配置文件编辑、MOD 预览，以及框架备份。
 *
 * 这三个域都很小，合在一个文件里按 `modConfig.` / `modPreview.` / `backup.`
 * 前缀分组；`index.ts` 组装时会按第一段拆回各自的域。
 */
export const modConfig: DomainMessages = {
  unsafePath: {
    'zh-CN': 'MOD 配置路径不安全',
    'zh-TW': 'MOD 設定路徑不安全',
    en: 'The MOD config path is unsafe',
    ko: 'MOD 설정 경로가 안전하지 않습니다',
    ja: 'MOD 設定のパスが安全ではありません',
  },
  notFound: {
    'zh-CN': 'MOD 配置文件不存在或无法访问',
    'zh-TW': 'MOD 設定檔不存在或無法存取',
    en: 'The MOD config file is missing or unreadable',
    ko: 'MOD 설정 파일이 없거나 접근할 수 없습니다',
    ja: 'MOD 設定ファイルが存在しないか、アクセスできません',
  },
  readFailed: {
    'zh-CN': '无法读取 MOD 配置文件',
    'zh-TW': '無法讀取 MOD 設定檔',
    en: 'Failed to read the MOD config file',
    ko: 'MOD 설정 파일을 읽지 못했습니다',
    ja: 'MOD 設定ファイルを読み込めませんでした',
  },
  tooLarge: {
    'zh-CN': 'MOD 配置文件超过 2 MiB，无法编辑',
    'zh-TW': 'MOD 設定檔超過 2 MiB，無法編輯',
    en: 'The MOD config file exceeds 2 MiB and cannot be edited',
    ko: 'MOD 설정 파일이 2 MiB를 초과하여 편집할 수 없습니다',
    ja: 'MOD 設定ファイルが 2 MiB を超えているため編集できません',
  },
  invalidUtf8: {
    'zh-CN': 'MOD 配置文件不是有效的 UTF-8 文本',
    'zh-TW': 'MOD 設定檔不是有效的 UTF-8 文字',
    en: 'The MOD config file is not valid UTF-8 text',
    ko: 'MOD 설정 파일이 올바른 UTF-8 텍스트가 아닙니다',
    ja: 'MOD 設定ファイルが有効な UTF-8 テキストではありません',
  },
  externallyModified: {
    'zh-CN': '配置文件已被外部修改，请重新加载后再保存',
    'zh-TW': '設定檔已被外部修改，請重新載入後再儲存',
    en: 'The config file was changed externally. Reload before saving.',
    ko: '설정 파일이 외부에서 변경되었습니다. 다시 불러온 후 저장하세요.',
    ja: '設定ファイルが外部で変更されています。再読み込みしてから保存してください。',
  },
  saveFailed: {
    'zh-CN': '无法保存 MOD 配置文件',
    'zh-TW': '無法儲存 MOD 設定檔',
    en: 'Failed to save the MOD config file',
    ko: 'MOD 설정 파일을 저장하지 못했습니다',
    ja: 'MOD 設定ファイルを保存できませんでした',
  },
}

export const modPreview: DomainMessages = {
  unknownType: {
    'zh-CN': '无法识别文件类型',
    'zh-TW': '無法辨識檔案類型',
    en: 'Unrecognized file type',
    ko: '파일 형식을 인식할 수 없습니다',
    ja: 'ファイルの種類を認識できません',
  },
  unsupportedType: {
    'zh-CN': '只支持 .dll 或 .zip 文件',
    'zh-TW': '僅支援 .dll 或 .zip 檔案',
    en: 'Only .dll or .zip files are supported',
    ko: '.dll 또는 .zip 파일만 지원합니다',
    ja: '.dll または .zip ファイルのみ対応しています',
  },
  expired: {
    'zh-CN': '预览已过期，请重新选择文件',
    'zh-TW': '預覽已過期，請重新選擇檔案',
    en: 'The preview expired. Select the file again.',
    ko: '미리보기가 만료되었습니다. 파일을 다시 선택하세요.',
    ja: 'プレビューの有効期限が切れました。ファイルを選び直してください。',
  },
  invalidId: {
    'zh-CN': '预览标识无效',
    'zh-TW': '預覽識別碼無效',
    en: 'Invalid preview identifier',
    ko: '미리보기 식별자가 올바르지 않습니다',
    ja: 'プレビュー識別子が無効です',
  },
}

export const backup: DomainMessages = {
  notFound: {
    'zh-CN': '备份目录不存在',
    'zh-TW': '備份目錄不存在',
    en: 'The backup directory does not exist',
    ko: '백업 디렉터리가 없습니다',
    ja: 'バックアップディレクトリが存在しません',
  },
  outsideRoot: {
    'zh-CN': '备份目录不在允许的备份位置',
    'zh-TW': '備份目錄不在允許的備份位置',
    en: 'The backup directory is not in an allowed backup location',
    ko: '백업 디렉터리가 허용된 백업 위치에 없습니다',
    ja: 'バックアップディレクトリが許可された場所にありません',
  },
  overlap: {
    'zh-CN': '备份与框架目录重叠，已停止操作以避免递归复制。请确认游戏目录不在管理器数据目录内，且管理器没有安装在 BepInEx 里。',
    'zh-TW': '備份與框架目錄重疊，已停止操作以避免遞迴複製。請確認遊戲目錄不在管理器資料目錄內，且管理器沒有安裝在 BepInEx 裡。',
    en: 'The backup and framework directories overlap, so the operation stopped to avoid recursive copying. Make sure the game directory is outside the manager data directory and the manager is not installed inside BepInEx.',
    ko: '백업과 프레임워크 디렉터리가 겹쳐 재귀 복사를 막기 위해 중단했습니다. 게임 디렉터리가 관리자 데이터 디렉터리 안에 없고 관리자가 BepInEx 안에 설치되지 않았는지 확인하세요.',
    ja: 'バックアップとフレームワークのディレクトリが重なっているため、再帰コピーを避けて中止しました。ゲームディレクトリがマネージャーのデータディレクトリ内にないこと、マネージャーが BepInEx 内にインストールされていないことを確認してください。',
  },
  sameDirectory: {
    'zh-CN': '备份目标与源目录是同一个目录，已停止操作以避免覆盖数据。',
    'zh-TW': '備份目標與來源目錄是同一個目錄，已停止操作以避免覆寫資料。',
    en: 'The backup target and the source are the same directory, so the operation stopped to avoid overwriting data.',
    ko: '백업 대상과 원본 디렉터리가 같아 데이터 덮어쓰기를 막기 위해 중단했습니다.',
    ja: 'バックアップ先とコピー元が同じディレクトリのため、データの上書きを避けるために中止しました。',
  },
  unavailable: {
    'zh-CN': '备份目录不可用',
    'zh-TW': '備份目錄無法使用',
    en: 'The backup directory is unavailable',
    ko: '백업 디렉터리를 사용할 수 없습니다',
    ja: 'バックアップディレクトリを使用できません',
  },
  wrongKind: {
    'zh-CN': '只能操作该框架的时间戳备份',
    'zh-TW': '只能操作該框架的時間戳備份',
    en: 'Only timestamped backups of this framework can be used',
    ko: '이 프레임워크의 타임스탬프 백업만 사용할 수 있습니다',
    ja: 'このフレームワークのタイムスタンプ付きバックアップのみ操作できます',
  },
  wrongKindBepInEx: {
    'zh-CN': '只能删除 BepInEx 时间戳备份',
    'zh-TW': '只能刪除 BepInEx 時間戳備份',
    en: 'Only timestamped BepInEx backups can be deleted',
    ko: 'BepInEx 타임스탬프 백업만 삭제할 수 있습니다',
    ja: 'BepInEx のタイムスタンプ付きバックアップのみ削除できます',
  },
  noReclaimableFiles: {
    'zh-CN': '未找到可回收的框架文件。备份已保存在 {path}',
    'zh-TW': '找不到可回收的框架檔案。備份已儲存在 {path}',
    en: 'No framework files to remove were found. The backup is at {path}',
    ko: '제거할 프레임워크 파일을 찾지 못했습니다. 백업은 {path}에 있습니다',
    ja: '削除できるフレームワークファイルが見つかりません。バックアップは {path} にあります',
  },
  noReclaimableFilesShort: {
    'zh-CN': '未找到可回收的框架文件。',
    'zh-TW': '找不到可回收的框架檔案。',
    en: 'No framework files to remove were found.',
    ko: '제거할 프레임워크 파일을 찾지 못했습니다.',
    ja: '削除できるフレームワークファイルが見つかりません。',
  },
  // 框架已恢复，但恢复前的暂存目录没能删除：{paths}
  restoreLeftoverCleanupFailed: {
    'zh-CN': '框架已恢复，但恢复前的暂存目录未能删除：{paths}。请手动删除，否则里面的 MOD 会被重复识别。',
    'zh-TW': '框架已還原，但還原前的暫存目錄未能刪除：{paths}。請手動刪除，否則裡面的 MOD 會被重複辨識。',
    en: 'The framework was restored, but the pre-restore staging folder could not be removed: {paths}. Delete it manually, otherwise its MODs will be detected twice.',
    ko: '프레임워크는 복원되었지만 복원 전 임시 폴더를 삭제하지 못했습니다: {paths}. 직접 삭제하세요. 그렇지 않으면 해당 MOD가 중복 인식됩니다.',
    ja: 'フレームワークは復元されましたが、復元前の一時フォルダーを削除できませんでした: {paths}。手動で削除してください。そうしないと中の MOD が重複して認識されます。',
  },
  recycleFailed: {
    'zh-CN': '文件回收失败，请尝试手动删除框架文件。（{reason}）备份已保存在 {path}',
    'zh-TW': '檔案回收失敗，請嘗試手動刪除框架檔案。（{reason}）備份已儲存在 {path}',
    en: 'Failed to move framework files to the recycle bin. Delete them manually. ({reason}) The backup is at {path}',
    ko: '프레임워크 파일을 휴지통으로 옮기지 못했습니다. 직접 삭제해 보세요. ({reason}) 백업은 {path}에 있습니다',
    ja: 'フレームワークファイルをゴミ箱へ移動できませんでした。手動で削除してください。（{reason}）バックアップは {path} にあります',
  },
  diyLevelRecycleFailed: {
    'zh-CN': '文件回收失败，请尝试手动删除 DIYLevel 文件。（{reason}）备份已保存在 {path}',
    'zh-TW': '檔案回收失敗，請嘗試手動刪除 DIYLevel 檔案。（{reason}）備份已儲存在 {path}',
    en: 'Failed to move DIYLevel files to the recycle bin. Delete them manually. ({reason}) The backup is at {path}',
    ko: 'DIYLevel 파일을 휴지통으로 옮기지 못했습니다. 직접 삭제해 보세요. ({reason}) 백업은 {path}에 있습니다',
    ja: 'DIYLevel ファイルをゴミ箱へ移動できませんでした。手動で削除してください。（{reason}）バックアップは {path} にあります',
  },
}
