import type { DomainMessages } from './types'
import { common } from './common.ts'

/**
 * dialogs 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const dialogs: DomainMessages = {
  readModNoteFirst: { 'zh-CN': '安装说明', 'zh-TW': '安裝說明', en: 'Installation notes', ko: '설치 안내', ja: 'インストール説明', de: "Installationsangaben", fr: "Notes d'installation", it: "Note di installazione", 'es-ES': "Notas de instalación", ru: "Заметки об установке", pl: "Notatki instalacyjne", 'pt-BR': "Notas de instalação" },
  packageReleaseNotes: { 'zh-CN': '更新日志', 'zh-TW': '更新日誌', en: 'Release notes', ko: '릴리스 노트', ja: '更新履歴', de: 'Versionshinweise', fr: 'Notes de version', it: 'Note di rilascio', 'es-ES': 'Notas de la versión', ru: 'Примечания к выпуску', pl: 'Informacje o wydaniu', 'pt-BR': 'Notas da versão' },
  continueAfterReading: { 'zh-CN': '继续', 'zh-TW': '繼續', en: 'Continue', ko: '계속', ja: '続行', de: "Weiterfahren", fr: "Continuer", it: "Continua", 'es-ES': "Continuar", ru: "Продолжать", pl: "Kontynuuj", 'pt-BR': "Continuar" },
  // 发现新版本
  updateAvailable: { 'zh-CN': '发现新版本', 'zh-TW': '發現新版本', en: 'Update available', ko: '새 버전 발견', ja: '新しいバージョン', de: "Update verfügbar", fr: "Mise à jour disponible", it: "Aggiornamento disponibile", 'es-ES': "Actualización disponible", ru: "Обновление доступно", pl: "Aktualizacja dostępna", 'pt-BR': "Atualizar disponível" },
  // 更新内容：
  whatSNew: { 'zh-CN': '更新内容：', 'zh-TW': '更新內容：', en: 'What\\\'s new:', ko: '업데이트 내용:', ja: '更新内容：', de: "Was ist neu:", fr: "Quoi de neuf?", it: "Cosa c'è di nuovo:", 'es-ES': "Qué es nuevo:", ru: "Что нового:", pl: "Co nowego?", 'pt-BR': "Quais são as novidades:" },
  // 本次更新没有提供说明。
  noReleaseNotesWereProvided: { 'zh-CN': '本次更新没有提供说明。', 'zh-TW': '本次更新沒有提供說明。', en: 'No release notes were provided.', ko: '이번 업데이트에는 설명이 없습니다.', ja: '今回の更新に説明はありません。', de: "Es wurden keine Release Notes vorgelegt.", fr: "Aucune note de mise en liberté n'a été fournie.", it: "Non sono state fornite note di rilascio.", 'es-ES': "No se proporcionaron notas de liberación.", ru: "Записки об освобождении не были предоставлены.", pl: "Nie dostarczono żadnych notatek do wydania.", 'pt-BR': "Nenhuma nota de lançamento foi fornecida." },
  // 当前 {current} → 新版 {next}
  text: { 'zh-CN': '当前 {current} → 新版 {next}', 'zh-TW': '目前 {current} → 新版 {next}', en: '{current} → {next}', ko: '{current} → {next}', ja: '{current} → {next}', de: "{current} → {next}", fr: "{current} → {next}", it: "{current} → {next}", 'es-ES': "{current} → {next}", ru: "{current} {next}", pl: "{current} → {next}", 'pt-BR': "{current} → {next}" },
  // 准备下载更新…
  preparingDownload: { 'zh-CN': '准备下载更新…', 'zh-TW': '準備下載更新…', en: 'Preparing download…', ko: '업데이트 다운로드 준비 중…', ja: '更新のダウンロードを準備中…', de: "Vorbereitung des Downloads…", fr: "Préparation du téléchargement…", it: "Preparare il download…", 'es-ES': "Preparando la descarga…", ru: "Подготовка к загрузке…", pl: "Przygotowanie pobierania…", 'pt-BR': "A preparar a transferência…" },
  // 更新安装已开始，应用将自动退出
  updateInstallStartedTheAppWillExitAutomatically: { 'zh-CN': '更新安装已开始，应用将自动退出', 'zh-TW': '更新安裝已開始，應用程式將自動結束', en: 'The update installer has started. The app will exit automatically.', ko: '업데이트 설치가 시작되었습니다. 앱이 자동으로 종료됩니다.', ja: '更新のインストールを開始しました。アプリは自動で終了します。', de: "Das Update-Installationsprogramm wurde gestartet. Das app wird automatisch beendet.", fr: "L'installateur de mise à jour a démarré. Le app sortira automatiquement.", it: "L'installatore di aggiornamento è iniziato. Il app uscirà automaticamente.", 'es-ES': "El instalador de actualización ha comenzado. El app saldrá automáticamente.", ru: "Запущен установщик обновлений. app выйдет автоматически.", pl: "Instalator aktualizacji już się rozpoczął. app automatycznie się wyłączy.", 'pt-BR': "O instalador de atualização foi iniciado. O app irá sair automaticamente." },
  // 更新
  update: common['action.update'],
  // 稍后
  later: { 'zh-CN': '稍后', 'zh-TW': '稍後', en: 'Later', ko: '나중에', ja: '後で', de: "Später", fr: "Plus tard", it: "Più tardi", 'es-ES': "Más tarde", ru: "Позже", pl: "Później", 'pt-BR': "Mais tarde" },
  // 当前已是最新版本
  youAreUpToDate: { 'zh-CN': '当前已是最新版本', 'zh-TW': '目前已是最新版本', en: 'You are up to date', ko: '최신 버전입니다', ja: '最新バージョンです', de: "Sie sind up to date", fr: "Vous êtes à jour", it: "Sei aggiornato", 'es-ES': "Estás al día", ru: "Ты в курсе событий", pl: "Jesteś na bieżąco", 'pt-BR': "Você está atualizado" },
  // 更新检查失败
  updateCheckFailed: { 'zh-CN': '更新检查失败', 'zh-TW': '檢查更新失敗', en: 'Update check failed', ko: '업데이트 확인 실패', ja: '更新確認に失敗しました', de: "Update-Prüfung fehlgeschlagen", fr: "La vérification de mise à jour a échoué", it: "Controllo dell'aggiornamento fallito", 'es-ES': "Control de actualización fallado", ru: "Проверка обновлений провалилась", pl: "Sprawdzanie aktualizacji nie powiodło się", 'pt-BR': "A verificação da atualização falhou" },
  // 发现新版本 {version}
  versionAvailable: { 'zh-CN': '发现新版本 {version}', 'zh-TW': '發現新版本 {version}', en: 'Version {version} available', ko: '{version} 버전이 있습니다', ja: 'バージョン {version} があります', de: "Version {version} verfügbar", fr: "Version {version} disponible", it: "Versione {version} disponibile", 'es-ES': "Versión {version} disponible", ru: "Версия {version} доступна", pl: "Wersja {version} dostępna", 'pt-BR': "Versão {version} disponível" },
  // 全部渠道检查失败，可前往发布页手动下载。
  allChannelsFailedDownloadManuallyFromTheReleasePage: { 'zh-CN': '全部渠道检查失败，可前往发布页手动下载。', 'zh-TW': '全部渠道檢查失敗，可前往發布頁手動下載。', en: 'All channels failed. Download manually from the release page.', ko: '모든 채널이 실패했습니다. 릴리스 페이지에서 직접 받으세요.', ja: 'すべての経路が失敗しました。リリースページから手動で取得してください。', de: "Alle Kanäle sind fehlgeschlagen. manuell von der Release-Seite herunterladen.", fr: "Tous les canaux ont échoué. Télécharger manuellement à partir de la page de publication.", it: "Tutti i canali sono falliti. Scarica manualmente dalla pagina di rilascio.", 'es-ES': "Todos los canales fallaron. Descarga manualmente desde la página de lanzamiento.", ru: "Все каналы вышли из строя. Скачайте вручную со страницы релиза.", pl: "Wszystkie kanały nie powiodły się. Pobierz ręcznie ze strony wydania.", 'pt-BR': "Todos os canais falharam. Transferir manualmente a partir da página de lançamento." },
  // 打开发布页
  openReleasePage: { 'zh-CN': '打开发布页', 'zh-TW': '開啟發佈頁', en: 'Open release page', de: 'Release-Seite öffnen', fr: 'Ouvrir la page de publication', it: 'Apri la pagina di rilascio', ko: '릴리스 페이지 열기', 'es-ES': 'Abrir la página de versiones', ru: 'Открыть страницу выпуска', ja: 'リリースページを開く', pl: 'Otwórz stronę wydania', 'pt-BR': 'Abrir página da versão' },
  // 未归类仓库
  uncategorizedRepositories: { 'zh-CN': '未归类仓库', 'zh-TW': '未歸類倉庫', en: 'Uncategorized repositories', ko: '미분류 저장소', ja: '未分類リポジトリ', de: "Nicht kategorisierte Repositories", fr: "Dépôts non classés", it: "Depositi non categorizzati", 'es-ES': "Repositorios no localizados", ru: "Некатегоризированные репозитории", pl: "Nieskategoryzowane repozytoria", 'pt-BR': "Repositórios sem categoria" },
  // 未分组
  ungrouped: { 'zh-CN': '未分组', 'zh-TW': '未分組', en: 'Ungrouped', ko: '미분류', ja: '未分類', de: "Nicht gruppiert", fr: "Non groupé", it: "Non raggruppati", 'es-ES': "No agrupados", ru: "негруппированный", pl: "Niezgrupowane", 'pt-BR': "Não agrupado" },
  // 最新版本
  latestVersion: { 'zh-CN': '最新版本', 'zh-TW': '最新版本', en: 'Latest version', ko: '최신 버전', ja: '最新バージョン', de: "Letzte Fassung", fr: "Dernière version", it: "Ultima versione", 'es-ES': "Última versión", ru: "Последняя версия", pl: "Najnowsza wersja", 'pt-BR': "Versão mais recente" },
  // 下载量
  downloads: { 'zh-CN': '下载量', 'zh-TW': '下載量', en: 'Downloads', ko: '다운로드', ja: 'ダウンロード数', de: "Downloaden", fr: "Téléchargements", it: "Download", 'es-ES': "Descargas", ru: "Скачать", pl: "Pliki do pobrania", 'pt-BR': "Transferências" },
  // 已就绪
  ready: { 'zh-CN': '已就绪', 'zh-TW': '已就緒', en: 'Ready', ko: '준비됨', ja: '準備完了', de: "Fertig", fr: "Prêt", it: "Pronti", 'es-ES': "Listo", ru: "Готовы", pl: "Gotowe", 'pt-BR': "Prontos" },
  // 社区仓库
  communityRepositories: { 'zh-CN': '社区仓库', 'zh-TW': '社群倉庫', en: 'Community repositories', ko: '커뮤니티 저장소', ja: 'コミュニティリポジトリ', de: "Gemeinschaftsspeicher", fr: "Dépôts communautaires", it: "Repertori comunitari", 'es-ES': "Repositorios comunitarios", ru: "Репозитории сообщества", pl: "Repozytoria wspólnotowe", 'pt-BR': "Repositórios comunitários" },
  // 来源未知
  unknownOrigin: { 'zh-CN': '来源未知', 'zh-TW': '來源未知', en: 'Unknown origin', ko: '출처 불명', ja: '出所未詳', de: "Unbekannter Ursprung", fr: "Origine inconnue", it: "Origine sconosciuta", 'es-ES': "Origen desconocido", ru: "Неизвестное происхождение", pl: "Nieznane pochodzenie", 'pt-BR': "Origem desconhecida" },
  // 搜索
  search: { 'zh-CN': '搜索', 'zh-TW': '搜尋', en: 'Search', ko: '검색', ja: '検索', de: "Suchen", fr: "Recherche", it: "Ricerca", 'es-ES': "Búsqueda", ru: "Поиск", pl: "Szukaj", 'pt-BR': "Procurar" },
  // 权限
  permissions: { 'zh-CN': '权限', 'zh-TW': '權限', en: 'Permissions', ko: '권한', ja: '権限', de: "Genehmigungen", fr: "Autorisations", it: "Permissioni", 'es-ES': "Permisos", ru: "Разрешения", pl: "Uprawnienia", 'pt-BR': "Permissões" },
  // 以管理员身份运行
  runAsAdministrator: { 'zh-CN': '以管理员身份运行', 'zh-TW': '以管理員身分執行', en: 'Run as administrator', ko: '관리자 권한으로 실행', ja: '管理者として実行', de: "Führen Sie als Administrator", fr: "Exécuter en tant qu'administrateur", it: "Eseguire come amministratore", 'es-ES': "Corre como administrador", ru: "Бегите как администратор", pl: "Uruchom jako administrator", 'pt-BR': "Executar como administrador" },
  // 当前已具备管理员权限
  administratorRightsAreActive: { 'zh-CN': '当前已具备管理员权限', 'zh-TW': '目前具備管理員權限', en: 'Administrator rights are active', ko: '관리자 권한이 있습니다', ja: '管理者権限があります', de: "Administratorrechte sind aktiv", fr: "Les droits d'administrateur sont actifs", it: "I diritti dell'amministratore sono attivi", 'es-ES': "Los derechos del administrador son activos", ru: "Права администратора активны", pl: "Prawa administratora są aktywne", 'pt-BR': "Os direitos de administrador estão ativos" },
  // 仅在游戏目录不可写时需要
  onlyNeededWhenTheGameFolderIsNotWritable: { 'zh-CN': '仅在游戏目录不可写时需要', 'zh-TW': '僅在遊戲目錄不可寫時需要', en: 'Only needed when the game folder is not writable', ko: '게임 폴더에 쓸 수 없을 때만 필요합니다', ja: 'ゲームフォルダーに書き込めない場合のみ必要です', de: "Nur erforderlich, wenn der Spielordner nicht beschreibbar ist", fr: "Seulement lorsque le dossier de jeu n'est pas enregistrable", it: "Solo necessario quando la cartella di gioco non è scrivibile", 'es-ES': "Sólo se necesita cuando la carpeta del juego no es writable", ru: "Нужна только тогда, когда папка игры не пишется", pl: "Wymagane tylko wtedy, gdy folder nie jest zapisywalny", 'pt-BR': "Só é necessário quando a pasta do jogo não for escrita" },
  // 已提权
  elevated: { 'zh-CN': '已提权', 'zh-TW': '已提權', en: 'Elevated', ko: '권한 있음', ja: '昇格済み', de: "erhöht", fr: "Élevé", it: "Elevato", 'es-ES': "Elevado", ru: "повышенный", pl: "Podwyższone", 'pt-BR': "Elevada" },
  // 普通权限
  standard: { 'zh-CN': '普通权限', 'zh-TW': '一般權限', en: 'Standard', ko: '일반 권한', ja: '通常権限', de: "Standardwert", fr: "Norme", it: "Standard standard", 'es-ES': "Estándar", ru: "Стандарт", pl: "Standardowy", 'pt-BR': "Padrão" },
  // 以管理员重启
  restartAsAdministrator: { 'zh-CN': '以管理员重启', 'zh-TW': '以管理員重新啟動', en: 'Restart as administrator', ko: '관리자 권한으로 재시작', ja: '管理者として再起動', de: "Neustart als Administrator", fr: "Redémarrer en tant qu'administrateur", it: "Riavviare come amministratore", 'es-ES': "Reiniciar como administrador", ru: "Перезагрузить как администратор", pl: "Przywróć jako administrator", 'pt-BR': "Reiniciar como administrador" },
  // 游戏目录可写
  gameFolderWritable: { 'zh-CN': '游戏目录可写', 'zh-TW': '遊戲目錄可寫', en: 'Game folder writable', ko: '게임 폴더 쓰기 가능', ja: 'ゲームフォルダー書き込み可', de: "Spielordner beschreibbar", fr: "Dossier de jeu enregistrable", it: "Cartella di gioco scrivibile", 'es-ES': "Carpeta de juego writable", ru: "Игровая папка Scritable", pl: "Katalog gier do zapisu", 'pt-BR': "Pasta do jogo com escrita" },
  // 可以直接安装和卸载 MOD
  modsCanBeInstalledAndRemovedDirectly: { 'zh-CN': '可以直接安装和卸载 MOD', 'zh-TW': '可以直接安裝和解除安裝 MOD', en: 'MODs can be installed and removed directly', ko: 'MOD를 바로 설치·제거할 수 있습니다', ja: 'MOD を直接インストール・削除できます', de: "MODs können direkt installiert und entfernt werden", fr: "Les MOD peuvent être installés et retirés directement", it: "MOD possono essere installati e rimossi direttamente", 'es-ES': "MODs se puede instalar y eliminar directamente", ru: "MOD могут быть установлены и удалены напрямую", pl: "MOD mogą być instalowane i usuwane bezpośrednio", 'pt-BR': "MODs podem ser instalados e removidos diretamente" },
  // 需要管理员权限才能写入
  administratorRightsAreRequiredToWrite: { 'zh-CN': '需要管理员权限才能写入', 'zh-TW': '需要管理員權限才能寫入', en: 'Administrator rights are required to write', ko: '쓰려면 관리자 권한이 필요합니다', ja: '書き込むには管理者権限が必要です', de: "Administratorrechte sind erforderlich, um zu schreiben", fr: "Les droits d'administrateur sont tenus de rédiger", it: "I diritti dell'amministratore sono tenuti a scrivere", 'es-ES': "Los derechos del administrador deben escribir", ru: "Права администратора должны быть написаны", pl: "Prawa administratora są wymagane do napisania", 'pt-BR': "Os direitos de administrador são necessários para escrever" },
  // macOS 上游戏目录不可写时的做法：改目录权限，而不是提权运行管理器
  fixFolderPermissionsInFinder: { 'zh-CN': '游戏目录不可写：在「访达」里给该目录（含 BepInEx）加上当前用户的读写权限', 'zh-TW': '遊戲目錄不可寫入：在「Finder」中為該目錄（含 BepInEx）加上目前使用者的讀寫權限', en: 'The game folder is not writable: in Finder, give your user read/write access to it (and BepInEx)', ko: '게임 폴더에 쓸 수 없습니다. Finder에서 해당 폴더(BepInEx 포함)에 현재 사용자의 읽기/쓰기 권한을 주세요', ja: 'ゲームフォルダーに書き込めません。Finder でこのフォルダー（BepInEx を含む）に現在のユーザーの読み書き権限を付けてください', de: "Der Spielordner ist nicht beschreibbar: Geben Sie Ihrem Benutzer im Finder Lese- / Schreibzugriff darauf (und BepInEx)", fr: "Le dossier de jeu n'est pas enregistrable : dans Finder, donnez à votre utilisateur un accès en lecture/écriture (et BepInEx)", it: "La cartella di gioco non è scrivibile: in Finder, dai al tuo utente di leggere / scrivere l'accesso a esso (e BepInEx)", 'es-ES': "La carpeta de juego no es obligatoria: en Finder, da acceso de lectura/escritura a su usuario (y BepInEx)", ru: "Папка игры не пишется: в Finder дайте пользователю доступ к ней для чтения / записи (и BepInEx)", pl: "Folder gry nie jest do zapisu: w Finderze, daj użytkownikowi dostęp do odczytu / zapisu (i BepInEx)", 'pt-BR': "A pasta do jogo não é gravável: no Finder, dê acesso de leitura/escrita ao seu usuário (e BepInEx)" },
  // 可写
  writable: { 'zh-CN': '可写', 'zh-TW': '可寫', en: 'Writable', ko: '쓰기 가능', ja: '書き込み可', de: "beschreibbar", fr: "Écrit", it: "Facilissimo", 'es-ES': "A título personal", ru: "письменный", pl: "Zapisywalne", 'pt-BR': "Escrita" },
  // 不可写
  notWritable: { 'zh-CN': '不可写', 'zh-TW': '不可寫', en: 'Not writable', ko: '쓰기 불가', ja: '書き込み不可', de: "nicht beschreibbar", fr: "Non justifiable", it: "Non scrivibile", 'es-ES': "No es necesario", ru: "Неписаный", pl: "Nie można zapisać", 'pt-BR': "Não gravável" },
  // 在 MOD 安装中查看
  viewInModInstall: { 'zh-CN': '在 MOD 安装中查看', 'zh-TW': '在 MOD 安裝中檢視', en: 'View in MOD Install', ko: 'MOD 설치에서 보기', ja: 'MOD インストールで表示', de: "Ansicht in MOD Install", fr: "Affichage dans l'installation MOD", it: "Visualizza in MOD Install", 'es-ES': "Vista en MOD Install", ru: "Посмотреть в MOD Install", pl: "Widok w instalacji MOD", 'pt-BR': "Ver na instalação do MOD" },
}
