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
}
