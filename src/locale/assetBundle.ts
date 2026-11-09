import type { DomainMessages } from './types'

/**
 * 地图（关卡集）`info` 解析错误。
 *
 * 这些 tag 只在「地图信息读不出来」时出现：文件不是资源包、资源包结构损坏、
 * 缺少地图信息对象、读文件本身失败。以前 `ParseError` 把技术细节拼在 tag
 * 后面（`assetBundle.corrupt:lzma: data error`），而这个域从头到尾没登记过
 * 语言表 —— 用户看到的就是那一串原始英文。现在细节走 `?reason=` 参数，
 * 固定文案在这里翻译。
 */
export const assetBundle: DomainMessages = {
  previewUnavailable: {'zh-CN':'无法解析关卡预览资源','zh-TW':'無法解析關卡預覽資源',en:'The level preview resource could not be read.',ko:'스테이지 미리보기 리소스를 읽을 수 없습니다.',ja:'ステージプレビューのリソースを読み取れません。'},
  notABundle: {
    'zh-CN': '这个文件不是 Unity 资源包（AssetBundle），读不出地图信息',
    'zh-TW': '這個檔案不是 Unity 資源包（AssetBundle），讀不出地圖資訊',
    en: 'This file is not a Unity asset bundle, so the map info cannot be read.',
    ko: '이 파일은 Unity 에셋 번들이 아니어서 지도 정보를 읽을 수 없습니다.',
    ja: 'このファイルは Unity アセットバンドルではないため、マップ情報を読み取れません。',
  },
  corrupt: {
    'zh-CN': '地图信息解析失败：{reason}',
    'zh-TW': '地圖資訊解析失敗：{reason}',
    en: 'Failed to parse the map info: {reason}',
    ko: '지도 정보를 해석하지 못했습니다: {reason}',
    ja: 'マップ情報の解析に失敗しました：{reason}',
  },
  missingLevelSetInfo: {
    'zh-CN': '资源包里没有地图信息对象（LevelSetInfoSO）',
    'zh-TW': '資源包裡沒有地圖資訊物件（LevelSetInfoSO）',
    en: 'The asset bundle has no map info object (LevelSetInfoSO).',
    ko: '에셋 번들에 지도 정보 오브젝트(LevelSetInfoSO)가 없습니다.',
    ja: 'アセットバンドルにマップ情報オブジェクト（LevelSetInfoSO）がありません。',
  },
  ioFailed: {
    'zh-CN': '读取地图文件失败：{reason}',
    'zh-TW': '讀取地圖檔案失敗：{reason}',
    en: 'Failed to read the map file: {reason}',
    ko: '지도 파일을 읽지 못했습니다: {reason}',
    ja: 'マップファイルの読み取りに失敗しました：{reason}',
  },
}
