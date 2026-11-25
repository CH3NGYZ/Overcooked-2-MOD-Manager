import type { DomainMessages } from './types'
import { common } from './common.ts'

/**
 * overview 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const overview: DomainMessages = {
  // 已安装 MOD
  installedMods: { 'zh-CN': '已安装 MOD', 'zh-TW': '已安裝 MOD', en: 'Installed MODs', ko: '설치된 MOD', ja: 'インストール済みMOD', de: "Installierte Mods", fr: "Mods installés", it: "Mod installati", 'es-ES': "Mods instalados", ru: "Установленные моды", pl: "Zainstalowane mody", 'pt-BR': "Mods instalados" },
  // 框架状态
  framework: { 'zh-CN': '框架状态', 'zh-TW': '框架狀態', en: 'Framework', ko: '프레임워크 상태', ja: 'フレームワーク状態', de: "Frameworks", fr: "Frameworks", it: "Framework", 'es-ES': "Entornos", ru: "Фреймворки", pl: "Frameworki", 'pt-BR': "Estruturas" },
  // 游戏路径
  gamePath: { 'zh-CN': '游戏路径', 'zh-TW': '遊戲路徑', en: 'Game path', ko: '게임 경로', ja: 'ゲームパス', de: "Spielpfad", fr: "Chemin du jeu", it: "Percorso di gioco", 'es-ES': "Camino del juego", ru: "Путь игры", pl: "Ścieżka do gry", 'pt-BR': "Caminho do jogo" },
  // 待处理事项
  pending: { 'zh-CN': '待处理事项', 'zh-TW': '待處理事項', en: 'Pending', ko: '처리 대기', ja: '保留項目', de: "Ausstehend", fr: "En attente", it: "Finanziamenti", 'es-ES': "Pendiente", ru: "В ожидании", pl: "Oczekiwanie", 'pt-BR': "Pendente" },
  // 已启用 {count} 个
  enabled: { 'zh-CN': '已启用 {count} 个', 'zh-TW': '已啟用 {count} 個', en: '{count} enabled', ko: '{count}개 활성', ja: '{count} 個有効', de: "{count} aktiviert", fr: "{count} activé", it: "{count} abilitato", 'es-ES': "{count} habilitado", ru: "{count} включен", pl: "Włącz {count}", 'pt-BR': "Activado o {count}" },
  // 正常
  ready: { 'zh-CN': '正常', 'zh-TW': '正常', en: 'Ready', ko: '정상', ja: '正常', de: "Fertig", fr: "Prêt", it: "Pronti", 'es-ES': "Listo", ru: "Готовы", pl: "Gotowe", 'pt-BR': "Prontos" },
  // 未安装
  notInstalled: { 'zh-CN': '未安装', 'zh-TW': '未安裝', en: 'Not installed', ko: '설치 안 됨', ja: '未インストール', de: "Nicht installiert", fr: "Non installé", it: "Non installato", 'es-ES': "No instalado", ru: "Не установлено", pl: "Nie zainstalowano", 'pt-BR': "Não instalado" },
  // 已验证
  verified: { 'zh-CN': '已验证', 'zh-TW': '已驗證', en: 'Verified', ko: '검증됨', ja: '検証済み', de: "Geprüft", fr: "Vérifié", it: "Verificato", 'es-ES': "Verificado", ru: "проверенный", pl: "Zweryfikowane", 'pt-BR': "Verificado" },
  // 目录异常
  pathInvalid: { 'zh-CN': '目录异常', 'zh-TW': '目錄異常', en: 'Path invalid', ko: '경로 오류', ja: 'パス異常', de: "Trassensperre", fr: "Voie non valide", it: "Percorso non valido", 'es-ES': "Patrón inválido", ru: "Путь неверный", pl: "Ścieżka nieprawidłowa", 'pt-BR': "Caminho inválido" },
  // 未设置
  notSet: { 'zh-CN': '未设置', 'zh-TW': '未設定', en: 'Not set', ko: '설정 안 됨', ja: '未設定', de: "Nicht eingestellt", fr: "Non défini", it: "Non impostato", 'es-ES': "No está listo", ru: "Не установлен", pl: "Nieustawione", 'pt-BR': "Não definido" },
  // 无法探测游戏目录
  cannotDetectGameDirectory: { 'zh-CN': '无法探测游戏目录', 'zh-TW': '無法偵測遊戲目錄', en: 'Cannot detect game directory', ko: '게임 폴더를 감지할 수 없습니다', ja: 'ゲームフォルダーを検出できません', de: "Kann kein Spielverzeichnis erkennen", fr: "Impossible de détecter le répertoire de jeu", it: "Non riesco a rilevare la directory di gioco", 'es-ES': "No puede detectar el directorio del juego", ru: "Не удается обнаружить директорию игры", pl: "Nie można wykryć katalogu gier", 'pt-BR': "Não foi possível detectar o diretório do jogo" },
  // 暂无可更新内容
  nothingToUpdate: { 'zh-CN': '暂无可更新内容', 'zh-TW': '暫無可更新內容', en: 'Nothing to update', ko: '업데이트할 항목 없음', ja: '更新はありません', de: "Nichts zu aktualisieren", fr: "Rien à mettre à jour", it: "Niente da aggiornare", 'es-ES': "Nada para actualizar", ru: "Нечего обновлять", pl: "Nic do aktualizacji", 'pt-BR': "Nada para atualizar" },
  // 有可更新的 MOD
  modUpdatesAvailable: { 'zh-CN': '有可更新的 MOD', 'zh-TW': '有可更新的 MOD', en: 'MOD updates available', ko: '업데이트 가능한 MOD 있음', ja: '更新可能なMODがあります', de: "MOD Updates verfügbar", fr: "Mises à jour du MOD disponibles", it: "Aggiornamenti MOD disponibili", 'es-ES': "Actualizaciones de MOD disponibles", ru: "Доступны обновления MOD", pl: "Dostępne aktualizacje MOD", 'pt-BR': "Atualizações do MOD disponíveis" },
  // 去更新
  review: { 'zh-CN': '去更新', 'zh-TW': '去更新', en: 'Review', ko: '업데이트하기', ja: '更新へ', de: "Überprüfung", fr: "Révision", it: "Recensione", 'es-ES': "Examen", ru: "Обзор", pl: "Przegląd", 'pt-BR': "Revisão" },
  // 去管理
  manage: { 'zh-CN': '去管理', 'zh-TW': '去管理', en: 'Manage', ko: '관리하기', ja: '管理へ', de: "Verwalten", fr: "Gérer", it: "Gestione", 'es-ES': "Gestionaje", ru: "Управлять", pl: "Zarządzanie", 'pt-BR': "Gerenciar" },
  // 去安装
  browse: { 'zh-CN': '去安装', 'zh-TW': '去安裝', en: 'Browse', ko: '설치하기', ja: 'インストールへ', de: "Browsen", fr: "Parcourir", it: "Sfoglia", 'es-ES': "Navega", ru: "просматривать", pl: "Przeglądaj", 'pt-BR': "Navegar" },
  // 在线 MOD
  onlineMods: { 'zh-CN': '在线 MOD', 'zh-TW': '線上 MOD', en: 'Online MODs', ko: '온라인 MOD', ja: 'オンラインMOD', de: "Online-Mods", fr: "Mods en ligne", it: "Mod online", 'es-ES': "Mods en línea", ru: "Моды в сети", pl: "Mody online", 'pt-BR': "Mods on-line" },
  // 本地 MOD
  localMods: { 'zh-CN': '本地 MOD', 'zh-TW': '本機 MOD', en: 'Local MODs', ko: '로컬 MOD', ja: 'ローカルMOD', de: "Lokale MODs", fr: "MODs locaux", it: "MODI locali", 'es-ES': "MOD locales", ru: "Локальные MOD", pl: "Lokalne MOD", 'pt-BR': "MODs locais" },
  // 在线地图
  onlineMaps: { 'zh-CN': '在线地图', 'zh-TW': '線上地圖', en: 'Online maps', ko: '온라인 지도', ja: 'オンライン地図', de: "Online-Karten", fr: "Cartes en ligne", it: "Mappa online", 'es-ES': "Mapas en línea", ru: "Онлайн карты", pl: "Mapy online", 'pt-BR': "Mapas online" },
  // 本地地图
  localMaps: { 'zh-CN': '本地地图', 'zh-TW': '本機地圖', en: 'Local maps', ko: '로컬 지도', ja: 'ローカル地図', de: "Lokale Karten", fr: "Cartes locales", it: "Mappe locali", 'es-ES': "Mapas locales", ru: "Местные карты", pl: "Lokalne mapy", 'pt-BR': "Mapas locais" },
  // ===== 列小标题：总览左右两条链路 =====
  // MOD 链路（框架 → 在线 → 本地）
  modsChain: { 'zh-CN': 'MOD 链路', 'zh-TW': 'MOD 鏈路', en: 'MOD chain', ko: 'MOD 체인', ja: 'MOD チェーン', de: "MOD-Kette", fr: "Chaîne MOD", it: "Catena MOD", 'es-ES': "cadena MOD", ru: "Сеть MOD", pl: "Łańcuch MOD", 'pt-BR': "Corrente MOD" },
  // 地图链路（框架 → 在线 → 本地）
  mapsChain: { 'zh-CN': '地图链路', 'zh-TW': '地圖鏈路', en: 'Map chain', ko: '지도 체인', ja: 'マップチェーン', de: "Kartenkette", fr: "Chaîne de cartes", it: "Mappa della catena", 'es-ES': "cadena de mapas", ru: "Карточная цепочка", pl: "Łańcuch mapy", 'pt-BR': "Cadeia de mapas" },
  // 去刷新
  refresh: { 'zh-CN': '去刷新', 'zh-TW': '去重新整理', en: 'Refresh', ko: '새로 고침', ja: '更新へ', de: "Aktualisieren", fr: "Actualiser", it: "Aggiorna", 'es-ES': "Actualizar", ru: "Обновить список", pl: "Odśwież", 'pt-BR': "Atualizar lista" },
  // 共 {count} 个 MOD · {failed} 个刷新失败
  modsFailedToRefresh: { 'zh-CN': '共 {count} 个 MOD · {failed} 个刷新失败', 'zh-TW': '共 {count} 個 MOD · {failed} 個重新整理失敗', en: '{count} MODs · {failed} failed to refresh', ko: 'MOD {count}개 · {failed}개 새로 고침 실패', ja: 'MOD {count} 件 · {failed} 件が更新失敗', de: "{count} MODs · {failed} konnte nicht aktualisiert werden", fr: "MODS {count} · {failed} n'a pas réussi à se rafraîchir", it: "{count} MODs · {failed} non è riuscito a rinfrescare", 'es-ES': "{count} MODs · {failed} no refrescó", ru: "{count} MODs · {failed} не удалось обновить", pl: "{count} MOD · {failed} nie odświeżał", 'pt-BR': "MODELOS {count} · {failed} falhou ao atualizar" },
  // 共 {count} 个 MOD · 全部正常
  modsAllHealthy: { 'zh-CN': '共 {count} 个 MOD · 全部正常', 'zh-TW': '共 {count} 個 MOD · 全部正常', en: '{count} MODs · all healthy', ko: 'MOD {count}개 · 모두 정상', ja: 'MOD {count} 件 · すべて正常', de: "{count} MODs · alle gesund", fr: "MODÈLES {count} · tous sains", it: "{count} MODs · tutti sani", 'es-ES': "{count} MODs · todos saludables", ru: "{count} MODs · все здоровые", pl: "{count} MOD · wszystkie zdrowe", 'pt-BR': "MODs {count} · todos saudáveis" },
  // 尚未加载任何 MOD
  noModsLoadedYet: { 'zh-CN': '尚未加载任何 MOD', 'zh-TW': '尚未載入任何 MOD', en: 'No MODs loaded yet', ko: '아직 불러온 MOD가 없습니다', ja: '読み込まれた MOD はまだありません', de: "Noch keine MODs geladen", fr: "Pas encore de MOD chargés", it: "Nessun MODs caricato ancora", 'es-ES': "No hay MODs cargados todavía", ru: "Никаких загрузок пока нет", pl: "Nie załadowano jeszcze MOD", 'pt-BR': "Nenhum MOD ainda carregado" },
  // 已安装 {count}
  installedCount: { 'zh-CN': '已安装 {count}', 'zh-TW': '已安裝 {count}', en: '{count} installed', ko: '{count}개 설치됨', ja: 'インストール済み {count} 件', de: "{count} installiert", fr: "{count} installé", it: "{count} installato", 'es-ES': "{count} instalado", ru: "{count} установлен", pl: "Zainstalowane {count}", 'pt-BR': "{count} instalado" },
  // 安装数量未知
  localCountUnavailable: { 'zh-CN': '安装数量未知', 'zh-TW': '安裝數量未知', en: 'Installed count unknown', ko: '설치 수를 알 수 없음', ja: 'インストール済み件数は不明', de: "Installierte Zählung unbekannt", fr: "Installé nombre inconnu", it: "Conto installato sconosciuto", 'es-ES': "Conteo instalado desconocido", ru: "Установленный счет неизвестный", pl: "Zainstalowana liczba nieznana", 'pt-BR': "Contagem instalada desconhecida" },
  // 共 {count} 张地图 · 全部正常
  mapsAllHealthy: { 'zh-CN': '共 {count} 张地图 · 全部正常', 'zh-TW': '共 {count} 張地圖 · 全部正常', en: '{count} maps · all healthy', ko: '지도 {count}개 · 모두 정상', ja: '地図 {count} 件 · すべて正常', de: "{count} Karten · alle gesund", fr: "Cartes {count} · toutes en bonne santé", it: "mappe {count} · tutte sane", 'es-ES': "Mapas {count} · todos saludables", ru: "Карты {count} · все здоровые", pl: "{count} mapy · wszystkie zdrowe", 'pt-BR': "Mapas {count} · todos saudáveis" },
  // 共 {count} 张地图 · {failed} 张刷新失败
  mapsFailedToRefresh: { 'zh-CN': '共 {count} 张地图 · {failed} 张有来源刷新失败', 'zh-TW': '共 {count} 張地圖 · {failed} 張有來源重新整理失敗', en: '{count} maps · {failed} with source refresh errors', ko: '지도 {count}개 · {failed}개의 소스 새로 고침 실패', ja: '地図 {count} 件 · うち {failed} 件でソース更新失敗', de: "{count}-Karten · {failed} mit Quellaktualisierungsfehlern", fr: "Cartes {count} · {failed} avec erreurs de mise à jour source", it: "{count} mappe · {failed} con errori di aggiornamento di origine", 'es-ES': "Mapas {count} · {failed} con errores de actualización fuente", ru: "Карты {count} · {failed} с ошибками обновления источника", pl: "{count} mapy · {failed} z błędami odświeżania źródeł", 'pt-BR': "Mapas {count} · {failed} com erros de atualização de fonte" },
  mapsCatalogUnconfirmed: { 'zh-CN': '已知 {count} 张地图 · 目录待确认', 'zh-TW': '已知 {count} 張地圖 · 目錄待確認', en: '{count} known maps · catalog unverified', ko: '확인된 지도 {count}개 · 카탈로그 미확인', ja: '確認済みの地図 {count} 件 · カタログ未確認', de: "{count} bekannte Karten · Katalog nicht verifiziert", fr: "{count} cartes connues · catalogue non vérifié", it: "{count} mappe conosciute · catalogo unverified", 'es-ES': "{count} mapas conocidos · catálogo noverificado", ru: "{count} известные карты · каталог непроверенных", pl: "{count} znane mapy · katalog niezweryfikowany", 'pt-BR': "{count} mapas conhecidos · catálogo não verificado" },
  // 尚未加载任何地图
  noMapsLoadedYet: { 'zh-CN': '尚未读取地图目录', 'zh-TW': '尚未讀取地圖目錄', en: 'Map catalog not loaded yet', ko: '지도 카탈로그를 아직 읽지 않았습니다', ja: '地図カタログは未読み込みです', de: "Kartenkatalog noch nicht geladen", fr: "Catalogue de cartes non encore chargé", it: "Catalogo della mappa non caricato ancora", 'es-ES': "Mapa catálogo no cargado todavía", ru: "Каталог карт еще не загружен", pl: "Katalog map jeszcze nie załadowany", 'pt-BR': "Catálogo do mapa ainda não carregado" },
  // 全部 MOD
  allMods: { 'zh-CN': '全部 MOD', 'zh-TW': '全部 MOD', en: 'All MODs', ko: '모든 MOD', ja: 'すべてのMOD', de: "Alle MODs", fr: "Tous les MOD", it: "Tutti i MOD", 'es-ES': "Todos los MOD", ru: "Все МОД", pl: "Wszystkie MOD", 'pt-BR': "Todos os MODs" },
  // 已启用 {enabled} / {total}
  ofEnabled: { 'zh-CN': '已启用 {enabled} / {total}', 'zh-TW': '已啟用 {enabled} / {total}', en: '{enabled} of {total} enabled', ko: '{total}개 중 {enabled}개 활성', ja: '{total} 件中 {enabled} 件が有効', de: "{enabled} von {total} aktiviert", fr: "{enabled} de {total} activé", it: "{enabled} di {total} abilitato", 'es-ES': "{enabled} de {total}", ru: "{enabled} {total} включен", pl: "Włączony {enabled} {total}", 'pt-BR': "{enabled} do {total} activado" },
  // 有 MOD 未启用
  someModsAreDisabled: { 'zh-CN': '有 MOD 未启用', 'zh-TW': '有 MOD 未啟用', en: 'Some MODs are disabled', ko: '비활성 MOD가 있습니다', ja: '無効な MOD があります', de: "Einige MODs sind deaktiviert", fr: "Certains MOD sont désactivés", it: "Alcuni MOD sono disabilitati", 'es-ES': "Algunos MOD están discapacitados", ru: "Некоторые MOD являются инвалидами", pl: "Niektóre MOD są wyłączone", 'pt-BR': "Alguns MODs estão desativados" },
  // 全部启用
  enableAll: { 'zh-CN': '全部启用', 'zh-TW': '全部啟用', en: 'Enable all', ko: '모두 활성화', ja: 'すべて有効化', de: "Alle aktivieren", fr: "Tout activer", it: "Abilita tutti", 'es-ES': "Activar todo", ru: "Включить всё", pl: "Włącz wszystkie", 'pt-BR': "Ativar todos" },
  // 全部禁用
  disableAll: { 'zh-CN': '全部禁用', 'zh-TW': '全部停用', en: 'Disable all', ko: '모두 비활성화', ja: 'すべて無効化', de: "Alle deaktivieren", fr: "Tout désactiver", it: "Disabilita tutti", 'es-ES': "Desactivar todo", ru: "Отключить всё", pl: "Wyłącz wszystkie", 'pt-BR': "Desativar todos" },
  // 禁用全部 MOD
  disableAllMods: { 'zh-CN': '禁用全部 MOD', 'zh-TW': '停用全部 MOD', en: 'Disable all MODs', ko: '모든 MOD 비활성화', ja: 'すべてのMODを無効化', de: "Deaktivieren Sie alle MODs", fr: "Désactiver tous les MOD", it: "Disattivare tutti i MOD", 'es-ES': "Desactivar todos los MOD", ru: "Отключить все MOD", pl: "Wyłącz wszystkie MOD", 'pt-BR': "Desactivar todos os MODs" },
  // 将禁用全部 {count} 个 MOD，游戏将不加载任何 MOD。可以随时重新启用。
  allModsWillBeDisabledTheGameWillLoadNoneOfThemYouCanReEnableThemAnytime: { 'zh-CN': '将禁用全部 {count} 个 MOD，游戏将不加载任何 MOD。可以随时重新启用。', 'zh-TW': '將停用全部 {count} 個 MOD，遊戲將不載入任何 MOD。可以隨時重新啟用。', en: 'All {count} MODs will be disabled; the game will load none of them. You can re-enable them anytime.', ko: 'MOD {count}개를 모두 비활성화합니다. 게임이 어떤 MOD도 불러오지 않습니다. 언제든 다시 활성화할 수 있습니다.', ja: 'MOD {count} 件をすべて無効化します。ゲームはどの MOD も読み込みません。いつでも再度有効化できます。', de: "Alle {count} MODs werden deaktiviert; das Spiel lädt keine von ihnen.", fr: "Tous les modèles {count} MODs seront désactivés; le jeu ne chargera aucun d'eux. Vous pouvez les réactiver à tout moment.", it: "Tutti i Mods {count} saranno disabilitati; il gioco non ne carica nessuno. È possibile riattivarli in qualsiasi momento.", 'es-ES': "Todos los MOD {count} serán deshabilitados; el juego no cargará ninguno de ellos. Puedes volver a habilitarlos en cualquier momento.", ru: "Все устройства {count} будут отключены; игра не будет загружать ни один из них. Вы можете повторно включить их в любое время.", pl: "Wszystkie {count} MOD będą wyłączone; gra nie załaduje żadnego z nich. Możesz je ponownie włączyć w każdej chwili.", 'pt-BR': "Todos os MODs {count} serão desativados; o jogo não carregará nenhum deles. Você poderá realizá- los novamente a qualquer momento." },
  // 安装后才能加载 MOD
  installItToLoadMods: { 'zh-CN': '安装后才能加载 MOD', 'zh-TW': '安裝後才能載入 MOD', en: 'Install it to load MODs', ko: '설치해야 MOD를 불러올 수 있습니다', ja: 'インストールしないとMODを読み込めません', de: "Installieren Sie es, um MODs zu laden", fr: "Installez-le pour charger les MOD", it: "Installarlo per caricare MODs", 'es-ES': "Instalarlo para cargar MODs", ru: "Установите его для загрузки MOD", pl: "Zainstaluj go, aby załadować MOD", 'pt-BR': "Instale-o para carregar MODs" },
  // 检测更新
  check: { 'zh-CN': '检测更新', 'zh-TW': '檢查更新', en: 'Check', ko: '확인', ja: '確認', de: "Überprüfung", fr: "Vérifier", it: "Controllare", 'es-ES': "Revisar", ru: "Проверить", pl: "Sprawdź", 'pt-BR': "Verificar" },
  // 只刷新已安装 MOD 绑定的仓库，检查是否有新版本
  refreshOnlyTheRepositoriesYourInstalledModsAreBoundToAndCheckForNewVersions: { 'zh-CN': '只刷新已安装 MOD 绑定的仓库，检查是否有新版本', 'zh-TW': '只重新整理已安裝 MOD 綁定的倉庫，檢查是否有新版本', en: 'Refresh only the repositories your installed MODs are bound to, and check for new versions', ko: '설치된 MOD가 연결된 저장소만 새로 고침해 새 버전이 있는지 확인합니다', ja: 'インストール済み MOD が紐づくリポジトリだけを更新し、新しいバージョンを確認します', de: "Aktualisieren Sie nur die Repositories, an die Ihre installierten MODs gebunden sind, und suchen Sie nach neuen Versionen", fr: "Actualiser seulement les dépôts de vos MOD installés sont liés à, et vérifier les nouvelles versions", it: "Rifiutare solo i repository che i vostri MOD installati sono tenuti a, e controllare per le nuove versioni", 'es-ES': "Refresh sólo los repositorios a los que sus MOD instalados están destinados, y comprobar nuevas versiones", ru: "Обновите только репозитории, к которым привязаны ваши установленные MOD, и проверьте новые версии", pl: "Odśwież tylko repozytoria, z którymi zainstalowane MOD są związane i sprawdź nowe wersje", 'pt-BR': "Atualizar apenas os repositórios aos quais seus MODs instalados estão vinculados e verificar novas versões" },
  // 没有已安装 MOD 绑定到可用的在线仓库
  noInstalledModIsBoundToAnAvailableOnlineRepository: { 'zh-CN': '没有已安装 MOD 绑定到可用的在线仓库', 'zh-TW': '沒有已安裝 MOD 綁定到可用的線上倉庫', en: 'No installed MOD is bound to an available online repository', ko: '사용 가능한 온라인 저장소에 연결된 설치된 MOD가 없습니다', ja: '利用可能なオンラインリポジトリに紐づくインストール済み MOD はありません', de: "Kein installiertes MOD ist an ein verfügbares Online-Repository gebunden", fr: "Aucun MOD installé n'est lié à un dépôt en ligne disponible", it: "Nessun MOD installato è legato ad un repository online disponibile", 'es-ES': "No instalado MOD está destinado a un repositorio en línea disponible", ru: "Ни один установленный MOD не привязан к доступному онлайн-хранилищу", pl: "Żaden zainstalowany MOD nie jest związany z dostępnym repozytorium online", 'pt-BR': "Nenhum MOD instalado está ligado a um repositório online disponível" },
  // 通用设置
  general: { 'zh-CN': '通用设置', 'zh-TW': '一般設定', en: 'General', ko: '일반 설정', ja: '一般設定', de: "Generaldirektion", fr: "Généralités", it: "Generale", 'es-ES': "Generalidades", ru: "Генерал", pl: "Uwagi ogólne", 'pt-BR': "Geral" },
  // 重装
  reinstall: common['action.reinstall'],
  // 启动游戏
  launchGame: { 'zh-CN': '启动游戏', 'zh-TW': '啟動遊戲', en: 'Launch game', ko: '게임 실행', ja: 'ゲームを起動', de: "Launch-Spiel", fr: "Lancer le jeu", it: "Gioco di lancio", 'es-ES': "Lanzamiento juego", ru: "Запуск игры", pl: "Uruchomienie gry", 'pt-BR': "Jogo de lançamento" },
  // 停止游戏
  stopGame: { 'zh-CN': '停止游戏', 'zh-TW': '停止遊戲', en: 'Stop game', ko: '게임 중지', ja: 'ゲームを停止', de: "Stop-Spiel", fr: "Arrêter le jeu", it: "Stop gioco", 'es-ES': "Parar juego", ru: "Хватит играть", pl: "Stop gry", 'pt-BR': "Parar o jogo" },
  // 关于
  about: { 'zh-CN': '关于', 'zh-TW': '關於', en: 'About', ko: '정보', ja: 'このアプリについて', de: "Über:", fr: "À propos", it: "Informazioni su", 'es-ES': "Acerca de", ru: "О нас", pl: "O", 'pt-BR': "Sobre" },
  // 当前路径
  currentPath: { 'zh-CN': '当前路径', 'zh-TW': '目前路徑', en: 'Current path', ko: '현재 경로', ja: '現在のパス', de: "Strompfad", fr: "Chemin actuel", it: "Percorso attuale", 'es-ES': "Carretera actual", ru: "Текущий путь", pl: "Bieżąca ścieżka", 'pt-BR': "Caminho atual" },
  // 选择或输入目录路径
  chooseOrEnterAFolderPath: { 'zh-CN': '选择或输入目录路径', 'zh-TW': '選擇或輸入目錄路徑', en: 'Choose or enter a folder path', ko: '폴더 경로를 선택하거나 입력하세요', ja: 'フォルダーパスを選択または入力', de: "Wählen oder betreten Sie einen Ordnerpfad", fr: "Choisissez ou entrez un chemin de dossier", it: "Scegliere o inserire un percorso di cartella", 'es-ES': "Elija o introduzca una ruta de carpeta", ru: "Выберите или введите путь папки", pl: "Wybierz lub wprowadź ścieżkę folderów", 'pt-BR': "Escolha ou insira um caminho de pasta" },
  // 浏览…
  browse2: { 'zh-CN': '浏览…', 'zh-TW': '瀏覽…', en: 'Browse…', ko: '찾아보기…', ja: '参照…', de: "Browsen…", fr: "Parcourir…", it: "Sfoglia…", 'es-ES': "Navega…", ru: "Просмотреть…", pl: "Przeglądaj…", 'pt-BR': "Navegar…" },
  // 确认保存
  save: { 'zh-CN': '确认保存', 'zh-TW': '確認儲存', en: 'Save', ko: '저장', ja: '保存', de: "Speichern", fr: "Enregistrer", it: "Salva", 'es-ES': "Guardar", ru: "Сохранить", pl: "Zapisz", 'pt-BR': "Salvar" },
  // 选择游戏目录
  chooseGameDirectory: { 'zh-CN': '选择游戏目录', 'zh-TW': '選擇遊戲目錄', en: 'Choose game directory', ko: '게임 폴더 선택', ja: 'ゲームフォルダーを選択', de: "Wählen Sie das Spielverzeichnis", fr: "Choisir le répertoire de jeu", it: "Scegli la directory di gioco", 'es-ES': "Elija el directorio del juego", ru: "Выберите игровой каталог", pl: "Wybierz katalog gier", 'pt-BR': "Escolha o diretório do jogo" },
  // 目录需包含 Overcooked!2.exe；保存后自动验证。
  theFolderMustContainOvercooked2ExeItIsVerifiedOnSave: { 'zh-CN': '目录需包含 Overcooked!2.exe；保存后自动验证。', 'zh-TW': '目錄需包含 Overcooked!2.exe；儲存後自動驗證。', en: 'The folder must contain Overcooked!2.exe; it is verified on save.', ko: '폴더에 Overcooked!2.exe가 있어야 하며 저장 후 검증합니다.', ja: 'Overcooked!2.exe を含む必要があります。保存時に検証します。', de: "Der Ordner muss Overcooked!2.exe enthalten; er wird auf save verifiziert.", fr: "Le dossier doit contenir Overcooked!2.exe; il est vérifié lors de l'enregistrement.", it: "La cartella deve contenere Overcooked!2.exe; è verificata su salvataggio.", 'es-ES': "La carpeta debe contener Overcooked!2.exe; se verifica en guardar.", ru: "Папка должна содержать Overcooked!2.exe; она проверяется на сохранение.", pl: "Folder musi zawierać Overcooked!2.exe; jest sprawdzany przy zapisie.", 'pt-BR': "A pasta deve conter Overcooked!2.exe; é verificada em salvar." },
}
