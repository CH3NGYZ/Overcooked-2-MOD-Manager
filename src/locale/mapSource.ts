import type { DomainMessages } from './types'

/**
 * 地图源（`maps.toml` 与子源）的校验错误。
 *
 * 源文档写错时用户只会看到这几条：地图条目缺字段、条目重复、找不到对应的
 * 地图或 Release。以前整个 `mapSource` 域没有登记语言表，用户看到的是
 * `mapSource.missingAssetRules` 这样的原始 tag。
 */
export const mapSource: DomainMessages = {
  mapMissing: {
    'zh-CN': '地图源里找不到这张地图',
    'zh-TW': '地圖來源裡找不到這張地圖',
    en: 'This map is not present in the map sources.',
    ko: '지도 소스에서 이 지도를 찾을 수 없습니다.',
    ja: 'マップソースにこのマップが見つかりません。',
  },
  releaseMissing: {
    'zh-CN': '找不到这张地图的最新版本信息',
    'zh-TW': '找不到這張地圖的最新版本資訊',
    en: 'No release information found for this map.',
    ko: '이 지도의 최신 버전 정보를 찾을 수 없습니다.',
    ja: 'このマップの最新バージョン情報が見つかりません。',
  },
  duplicateEntry: {
    'zh-CN': '地图源里有重复条目：{identity}',
    'zh-TW': '地圖來源裡有重複條目：{identity}',
    en: 'Duplicate entry in the map sources: {identity}',
    ko: '지도 소스에 중복된 항목이 있습니다: {identity}',
    ja: 'マップソースに重複した項目があります：{identity}',
  },
  missingLevelSetName: {
    'zh-CN': '地图条目缺少关卡集名称：{map}',
    'zh-TW': '地圖條目缺少關卡集名稱：{map}',
    en: 'Map entry is missing its level set name: {map}',
    ko: '지도 항목에 레벨 세트 이름이 없습니다: {map}',
    ja: 'マップ項目にレベルセット名がありません：{map}',
  },
  missingCoordinates: {
    'zh-CN': '地图条目缺少作者或仓库：{map}',
    'zh-TW': '地圖條目缺少作者或倉庫：{map}',
    en: 'Map entry is missing its author or repository: {map}',
    ko: '지도 항목에 작성자 또는 저장소가 없습니다: {map}',
    ja: 'マップ項目に作者またはリポジトリがありません：{map}',
  },
  missingAssetRules: {
    'zh-CN': '地图条目缺少安装包规则：{map}',
    'zh-TW': '地圖條目缺少安裝包規則：{map}',
    en: 'Map entry is missing its asset rules: {map}',
    ko: '지도 항목에 설치 파일 규칙이 없습니다: {map}',
    ja: 'マップ項目にアセット規則がありません：{map}',
  },
  assetRuleNeedsVersion: {
    'zh-CN': '固定 Tag 的地图条目缺少可提取版本号的正则规则：{map}',
    'zh-TW': '固定 Tag 的地圖條目缺少可擷取版本號的正規表示式規則：{map}',
    en: 'A map entry with a fixed tag needs a regex rule that captures the version: {map}',
    ko: '고정 태그 지도 항목에는 버전을 추출하는 정규식 규칙이 필요합니다: {map}',
    ja: '固定タグのマップ項目にはバージョンを抽出する正規表現ルールが必要です：{map}',
  },
  releaseTagNotFound: {
    'zh-CN': '地图条目指定的 Tag 不存在：{map}（{tag}）',
    'zh-TW': '地圖條目指定的 Tag 不存在：{map}（{tag}）',
    en: 'The tag configured for this map does not exist: {map} ({tag})',
    ko: '지도 항목에 지정된 태그가 없습니다: {map} ({tag})',
    ja: 'マップ項目に指定されたタグが存在しません：{map}（{tag}）',
  },
  noAssetInTag: {
    'zh-CN': '该 Tag 下没有匹配的安装包：{map}（{tag}）',
    'zh-TW': '該 Tag 下沒有符合的安裝包：{map}（{tag}）',
    en: 'No matching asset was found in that tag: {map} ({tag})',
    ko: '해당 태그에 일치하는 설치 파일이 없습니다: {map} ({tag})',
    ja: 'そのタグに一致するアセットがありません：{map}（{tag}）',
  },
  duplicateVersion: {
    'zh-CN': '地图条目的版本号重复：{map}（{version}）',
    'zh-TW': '地圖條目的版本號重複：{map}（{version}）',
    en: 'Duplicate version for this map entry: {map} ({version})',
    ko: '지도 항목의 버전 번호가 중복됩니다: {map} ({version})',
    ja: 'マップ項目のバージョン番号が重複しています：{map}（{version}）',
  },
  latestVersionMissing: {
    'zh-CN': '指定的最新版本在安装包里找不到：{map}（{version}）',
    'zh-TW': '指定的最新版本在安裝包裡找不到：{map}（{version}）',
    en: 'The declared latest version is not present in the assets: {map} ({version})',
    ko: '지정한 최신 버전을 설치 파일에서 찾을 수 없습니다: {map} ({version})',
    ja: '指定された最新バージョンがアセットに見つかりません：{map}（{version}）',
  },
  assetDocumentSubSources: {
    'zh-CN': '托管在 Release 资产里的地图源文档不能声明子源：{source}',
    'zh-TW': '託管在 Release 資產裡的地圖來源文件不能宣告子來源：{source}',
    en: 'A map source document hosted in a release asset may not declare child sources: {source}',
    ko: '릴리스 자산에 호스팅된 지도 소스 문서는 하위 소스를 선언할 수 없습니다: {source}',
    ja: 'Release アセットで配布されるマップソース文書は子ソースを宣言できません：{source}',
  },
  regexNotAnchored: {
    'zh-CN': '安装包规则的正则必须用 ^ 和 $ 锚定：{map}（{asset}）',
    'zh-TW': '安裝包規則的正規表示式必須用 ^ 與 $ 錨定：{map}（{asset}）',
    en: 'The asset rule regex must be anchored with ^ and $: {map} ({asset})',
    ko: '설치 파일 규칙의 정규식은 ^와 $로 고정해야 합니다: {map} ({asset})',
    ja: 'アセット規則の正規表現は ^ と $ で固定する必要があります：{map}（{asset}）',
  },
}
