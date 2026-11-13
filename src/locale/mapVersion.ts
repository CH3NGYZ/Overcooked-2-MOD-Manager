import type { DomainMessages } from './types'

/** Multi-source map version evidence and explicit install choices. */
export const mapVersion: DomainMessages = {
  sourceLatestLabel: { 'zh-CN': '本源最新版本', 'zh-TW': '本來源最新版本', en: 'Latest from this source', ko: '이 소스의 최신 버전', ja: 'このソースの最新バージョン' },
  sameVersion: { 'zh-CN': '与本地版本相同', 'zh-TW': '與本機版本相同', en: 'Same version as installed', ko: '설치된 버전과 동일', ja: 'インストール済みと同じバージョン' },
  localNewer: { 'zh-CN': '本地版本较新', 'zh-TW': '本機版本較新', en: 'Installed version is newer', ko: '설치된 버전이 더 최신', ja: 'インストール済みの方が新しい' },
  incomparable: { 'zh-CN': '已安装，版本不可比', 'zh-TW': '已安裝，版本無法比較', en: 'Installed; versions cannot be compared', ko: '설치됨 · 버전 비교 불가', ja: 'インストール済み・バージョン比較不可' },
  installThisVersion: { 'zh-CN': '安装此版本', 'zh-TW': '安裝此版本', en: 'Install this version', ko: '이 버전 설치', ja: 'このバージョンをインストール' },
  chooseThisVersion: { 'zh-CN': '主动安装此版本', 'zh-TW': '主動安裝此版本', en: 'Choose this version', ko: '이 버전을 선택하여 설치', ja: 'このバージョンを選んでインストール' },
  sourceLatest: { 'zh-CN': '本源最新版本：{version}', 'zh-TW': '本來源最新版本：{version}', en: 'Latest from this source: {version}', ko: '이 소스의 최신 버전: {version}', ja: 'このソースの最新バージョン：{version}' },
  defaultSource: { 'zh-CN': '默认更新来源', 'zh-TW': '預設更新來源', en: 'Default update source', ko: '기본 업데이트 소스', ja: '既定の更新元' },
  otherSources: { 'zh-CN': '其他更新来源', 'zh-TW': '其他更新來源', en: 'Other update sources', ko: '다른 업데이트 소스', ja: '他の更新元' },
}
