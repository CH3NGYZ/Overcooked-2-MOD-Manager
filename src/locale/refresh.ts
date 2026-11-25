import type { DomainMessages } from './types'

/**
 * 刷新批次的共享文案（MOD 与地图两侧同一套）。
 *
 * 为什么不放进 `modsLocal` / `levelSet` 各自一张表：同一批刷新会在两个页面
 * 显示（管理页的摘要标签、下载页的浮动提示），两页各写一条文案必然写得不一样，
 * 用户会以为它们说的不是一回事。这里只差一个量词（个仓库 / 张地图），
 * 其余措辞共用。
 */
export const refresh: DomainMessages = {
  // 已检查 {count} 个仓库
  checkedRepositories: { 'zh-CN': '已检查 {count} 个仓库', 'zh-TW': '已檢查 {count} 個倉庫', en: 'Checked {count} repositories', ko: '{count}개 저장소를 확인했습니다', ja: '{count} 個のリポジトリを確認しました', de: "Geprüfte {count}-Repositorys", fr: "Dépôts de {count} vérifiés", it: "Repositori {count} controllati", 'es-ES': "Repositorios {count}", ru: "Проверенные репозитории {count}", pl: "Skontrolowane repozytoria {count}", 'pt-BR': "Repositórios {count} verificados" },
  // 已检查 {count} 张地图
  checkedMaps: { 'zh-CN': '已检查 {count} 张地图', 'zh-TW': '已檢查 {count} 張地圖', en: 'Checked {count} maps', ko: '{count}개 지도를 확인했습니다', ja: '{count} 個のマップを確認しました', de: "Geprüfte {count}-Karten", fr: "A vérifié les cartes de {count}", it: "Mappe {count} controllate", 'es-ES': "Mapas de {count}", ru: "Проверенные карты {count}", pl: "Sprawdzone mapy {count}", 'pt-BR': "Mapas {count} verificados" },
  // {count} 个有更新
  updated: { 'zh-CN': '{count} 个有更新', 'zh-TW': '{count} 個有更新', en: '{count} with updates', ko: '{count}개 업데이트 있음', ja: '{count} 件に更新あり', de: "{count} mit Updates", fr: "{count} avec mises à jour", it: "{count} con aggiornamenti", 'es-ES': "{count} con actualizaciones", ru: "{count} с обновлениями", pl: "{count} z aktualizacjami", 'pt-BR': "{count} com atualizações" },
  // {count} 个失败
  failed: { 'zh-CN': '{count} 个失败', 'zh-TW': '{count} 個失敗', en: '{count} failed', ko: '{count}개 실패', ja: '{count} 件失敗', de: "{count} gescheitert", fr: "{count} a échoué", it: "{count} fallito", 'es-ES': "{count} falló", ru: "{count} провалился", pl: "{count} nie powiodło się", 'pt-BR': "Falha no {count}" },
  // 上次检查
  lastCheck: { 'zh-CN': '上次检查', 'zh-TW': '上次檢查', en: 'Last check', ko: '마지막 확인', ja: '前回の確認', de: "Letzte Kontrolle", fr: "Dernier contrôle", it: "Ultimo controllo", 'es-ES': "Último cheque", ru: "Последняя проверка", pl: "Ostatnia kontrola", 'pt-BR': "Última verificação" },
  // 失败原因
  failureReasons: { 'zh-CN': '失败原因', 'zh-TW': '失敗原因', en: 'Failure reasons', ko: '실패 원인', ja: '失敗した理由', de: "Fehlergründe", fr: "Défaut de motivation", it: "Motivi di errore", 'es-ES': "Razones de fracaso", ru: "Причины неудачи", pl: "Powody niepowodzenia", 'pt-BR': "Motivos de falha" },
}
