import type { DomainMessages } from '../types.ts'

/** 上传确认、附件命名、传输进度与上传错误。同一 key 的五语保持集中。 */
export const mapPublishUpload: DomainMessages = {
  assetExists: {
    'zh-CN': '附件 {name} 已存在，请先处理已有附件或选择新版本。',
    'zh-TW': '附件 {name} 已存在，請先處理現有附件或選擇新版本。',
    en: 'Asset {name} already exists. Resolve the existing asset or choose a new version.',
    ko: '첨부 파일 {name}이 이미 있습니다. 기존 파일을 처리하거나 새 버전을 선택하세요.',
    ja: '添付ファイル {name} は既に存在します。既存のファイルを確認するか、新しいバージョンを選択してください。',
  },
  assets: {
    'zh-CN': '附件',
    'zh-TW': '附件',
    en: 'Assets',
    ko: '첨부 파일',
    ja: '添付ファイル',
  },
  assetsHint: {
    'zh-CN': '地图 ZIP 的下载地址从这里的附件匹配；保存元数据后才能上传。',
    'zh-TW': '地圖 ZIP 的下載網址從這裡的附件匹配；儲存中繼資料後才能上傳。',
    en: 'Map ZIPs are matched against these assets; save the metadata before uploading.',
    ko: '지도 ZIP 다운로드 주소는 이 첨부 파일에서 찾습니다. 메타데이터를 저장한 뒤 업로드하세요.',
    ja: 'マップ ZIP のダウンロード元はここの添付ファイルから決まります。メタデータを保存してからアップロードしてください。',
  },
  assetEmpty: {
    'zh-CN': '这个版本还没有附件。',
    'zh-TW': '這個版本還沒有附件。',
    en: 'This release has no assets yet.',
    ko: '이 릴리스에는 아직 첨부 파일이 없습니다.',
    ja: 'このリリースにはまだ添付ファイルがありません。',
  },
  uploaded: {
    'zh-CN': '附件已上传',
    'zh-TW': '附件已上傳',
    en: 'Asset uploaded',
    ko: '첨부 파일이 업로드되었습니다',
    ja: '添付ファイルをアップロードしました',
  },
  uploadHint: {
    'zh-CN': '只能上传 ZIP，最大 256 MB；版本号从 ZIP 内的 info 读取，上传时自动按规范生成附件名。',
    'zh-TW': '只能上傳 ZIP，最大 256 MB；版本號從 ZIP 內的 info 讀取，上傳時自動依規範產生附件名稱。',
    en: 'ZIP only, up to 256 MB. The version is read from info inside the ZIP, and the asset name is normalized automatically when uploading.',
    ko: 'ZIP만 업로드할 수 있으며 최대 256 MB입니다. 버전은 ZIP 내부 info에서 읽고 업로드 시 첨부 파일 이름을 규칙에 맞게 자동 생성합니다.',
    ja: 'ZIP のみ、最大 256 MB。バージョンは ZIP 内の info から読み取り、アップロード時に規則に沿った添付ファイル名を自動生成します。',
  },

  zipNameOk: {
    'zh-CN': '文件名符合规则',
    'zh-TW': '檔案名稱符合規則',
    en: 'File name matches the rule',
    ko: '파일 이름이 규칙에 맞습니다',
    ja: 'ファイル名は規則に一致しています',
  },
  zipNameNeedsRename: {
    'zh-CN': '上传时将自动规范附件名为 {name}',
    'zh-TW': '上傳時將自動規範附件名稱為 {name}',
    en: 'The asset name will be normalized automatically to {name} when uploading',
    ko: '업로드 시 첨부 파일 이름이 {name}(으)로 자동 생성됩니다',
    ja: 'アップロード時に添付ファイル名を {name} に自動で統一します',
  },
  zipNameWillUploadAs: {
    'zh-CN': '上传后附件名为 {name}',
    'zh-TW': '上傳後附件名為 {name}',
    en: 'The asset will be uploaded as {name}',
    ko: '업로드 후 첨부 파일 이름은 {name}입니다',
    ja: 'アップロード後の添付ファイル名は {name} です',
  },
  zipUploadBlocked: {
    'zh-CN': '文件名改成 {name} 后才能上传',
    'zh-TW': '檔案名稱改成 {name} 後才能上傳',
    en: 'Rename the file to {name} before uploading',
    ko: '파일 이름을 {name}(으)로 바꾼 뒤 업로드할 수 있습니다',
    ja: 'ファイル名を {name} に変更するとアップロードできます',
  },
  zipIdentityMismatch: {
    'zh-CN': 'ZIP 中的地图英文名、中文名或作者与已有地图信息不一致。为避免安装识别和更新检测错误，已阻止上传，请确认 info 元数据是否变更。',
    'zh-TW': 'ZIP 中的地圖英文名、中文名或作者與現有地圖資訊不一致。為避免安裝辨識和更新偵測錯誤，已阻止上傳，請確認 info 中繼資料是否變更。',
    en: 'The English name, Chinese name or author in the ZIP does not match the existing map. Uploading has been blocked to prevent installation identification and update detection errors. Check whether the info metadata has changed.',
    ko: 'ZIP의 지도 영문 이름, 중국어 이름 또는 작성자가 기존 지도 정보와 일치하지 않습니다. 설치 인식 및 업데이트 감지 오류를 방지하기 위해 업로드가 차단되었습니다. info 메타데이터가 변경되었는지 확인하세요.',
    ja: 'ZIP 内のマップの英語名、中国語名、または作者が既存のマップ情報と一致しません。インストール時の識別や更新検出のエラーを防ぐため、アップロードをブロックしました。info メタデータが変更されていないか確認してください。',
  },
  uploading: {
    'zh-CN': '正在上传',
    'zh-TW': '正在上傳',
    en: 'Uploading',
    ko: '업로드 중',
    ja: 'アップロード中',
  },
  uploadConfirming: {
    'zh-CN': '等待服务器确认',
    'zh-TW': '等待伺服器確認',
    en: 'Waiting for server confirmation',
    ko: '서버 확인 대기 중',
    ja: 'サーバーの確認を待っています',
  },
  uploadProgress: {
    'zh-CN': '上传进度',
    'zh-TW': '上傳進度',
    en: 'Upload progress',
    ko: '업로드 진행률',
    ja: 'アップロードの進行状況',
  },
  invalidAsset: {
    'zh-CN': '附件不合法：{reason}',
    'zh-TW': '附件不合法：{reason}',
    en: 'Invalid asset: {reason}',
    ko: '잘못된 첨부 파일입니다: {reason}',
    ja: '添付ファイルが正しくありません：{reason}',
  },
  assetTooLarge: {
    'zh-CN': '附件超过 {limit} MB 上限',
    'zh-TW': '附件超過 {limit} MB 上限',
    en: 'The asset exceeds the {limit} MB limit',
    ko: '첨부 파일이 {limit} MB 제한을 초과합니다',
    ja: '添付ファイルが上限の {limit} MB を超えています',
  },

  stepAssetTitle: {
    'zh-CN': "上传确认",
    'zh-TW': "上傳確認",
    en: "Upload confirmation",
    ko: "업로드 확인",
    ja: "アップロード確認",
  },

  continueToUpload: {
    'zh-CN': "下一步：上传确认",
    'zh-TW': "下一步：上傳確認",
    en: "Next: upload confirmation",
    ko: "다음: 업로드 확인",
    ja: "次へ：アップロード確認",
  },

  confirmUpload: {
    'zh-CN': "确认并上传",
    'zh-TW': "確認並上傳",
    en: "Confirm and upload",
    ko: "확인 후 업로드",
    ja: "確認してアップロード",
  },

  uploadConfirmationHint: {
    'zh-CN': "核对上述信息后再上传：此时才创建或复用 Release。上传成功后仍需在版本管理中确认发布，才完成登记。",
    'zh-TW': "核對上述資訊後再上傳：此時才建立或重用 Release。上傳成功後仍須在版本管理中確認發布，才完成登記。",
    en: "Review before uploading. The release is created or reused only now. After upload, confirm publication in version management to register it.",
    ko: "정보를 확인한 후 업로드하세요. 이때 릴리스를 생성하거나 재사용합니다. 업로드 후 버전 관리에서 게시를 확인해야 등록됩니다.",
    ja: "内容を確認してアップロードしてください。この時点で Release を作成または再利用します。アップロード後、バージョン管理で公開を確定すると登録完了です。",
  },
}
