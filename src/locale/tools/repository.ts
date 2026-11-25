import type { DomainMessages } from '../types.ts'

/** 仓库详情、版本选择、刷新状态与下载进度。 */
export const toolsUiRepository: DomainMessages = {
  downloadReceived: { 'zh-CN': '已下载 {amount}', 'zh-TW': '已下載 {amount}', en: 'Downloaded {amount}', ko: '{amount} 다운로드됨', ja: '{amount} ダウンロード済み', de: 'Heruntergeladen: {amount}', fr: 'Téléchargé {amount}', it: 'Scaricato {amount}', 'es-ES': 'Descargado {amount}', ru: 'Скачано {amount}', pl: 'Pobrano {amount}', 'pt-BR': 'Baixado: {amount}' },
  downloadProgress: { 'zh-CN': '下载进度', 'zh-TW': '下載進度', en: 'Download progress', ko: '다운로드 진행률', ja: 'ダウンロードの進行状況', de: "Download Fortschritte", fr: "Télécharger les progrès", it: "Scarica i progressi", 'es-ES': "Descargar progreso", ru: "Скачать Progress", pl: "Postępy pobierania", 'pt-BR': "Transferir o progresso" },
  downloading: { 'zh-CN': '正在下载', 'zh-TW': '正在下載', en: 'Downloading', ko: '다운로드 중', ja: 'ダウンロード中', de: "Download", fr: "Téléchargement", it: "Scaricamento", 'es-ES': "Descargar", ru: "Загрузить", pl: "Pobieranie", 'pt-BR': "Baixando" },
  mergingVolumes: { 'zh-CN': '正在重组并校验分卷', 'zh-TW': '正在重組並校驗分卷', en: 'Reassembling and verifying volumes', ko: '분할 파일 병합 및 검증 중', ja: '分割ファイルの再構築と検証中', de: "Wiedermontage und Überprüfung von Volumen", fr: "Réassembler et vérifier les volumes", it: "Rimontaggio e verifica dei volumi", 'es-ES': "Reagrupamiento y verificación de volúmenes", ru: "Сборка и проверка объемов", pl: "Wielkości ponownego montażu i weryfikacji", 'pt-BR': "Remontar e verificar os volumes" },
  volumeSummary: { 'zh-CN': '分卷：已完成 {done}/{total}', 'zh-TW': '分卷：已完成 {done}/{total}', en: 'Volumes: {done}/{total} completed', ko: '분할 파일: {done}/{total} 완료', ja: '分割ファイル：{done}/{total} 完了', de: "Bände: {done}/{total} fertiggestellt", fr: "Volumes: {done}/{total} terminés", it: "Volumes: {done}/{total} completato", 'es-ES': "Volumen: {done}/{total} completado", ru: "Объемы: {done}/{total} завершен", pl: "Wolumen: {done} / {total} zakończony", 'pt-BR': "Volumes: {done}/{total} completado" },
  volumeRetry: { 'zh-CN': '整卷重试 {count}/3', 'zh-TW': '整卷重試 {count}/3', en: 'Volume retries {count}/3', ko: '분할 파일 재시도 {count}/3', ja: '分割ファイル再試行 {count}/3', de: "Volumenversuche {count}/3", fr: "Volumes de recherche {count}/3", it: "Rettori del volume {count}/3", 'es-ES': "Volúmenes {count}/3", ru: "Объемные повторы {count}/3", pl: "Ponowne próby głośności {count} / 3", 'pt-BR': "Repetições de volume {count}/3" },
  'volumeState.pending': { 'zh-CN': '排队中', 'zh-TW': '排隊中', en: 'Queued', ko: '대기 중', ja: '待機中', de: "In Warteschlange", fr: "En file d’attente", it: "In coda", 'es-ES': "En cola", ru: "В очереди", pl: "W kolejce", 'pt-BR': "Na fila" },
  'volumeState.downloading': { 'zh-CN': '下载中', 'zh-TW': '下載中', en: 'Downloading', ko: '다운로드 중', ja: 'ダウンロード中', de: "Download", fr: "Téléchargement", it: "Scaricamento", 'es-ES': "Descargar", ru: "Загрузить", pl: "Pobieranie", 'pt-BR': "Baixando" },
  'volumeState.completed': { 'zh-CN': '已完成', 'zh-TW': '已完成', en: 'Completed', ko: '완료', ja: '完了', de: "Abgeschlossen", fr: "Achevé", it: "Completo", 'es-ES': "Completado", ru: "Завершено", pl: "Zakończono", 'pt-BR': "Concluído" },
  'volumeState.failed': { 'zh-CN': '失败', 'zh-TW': '失敗', en: 'Failed', ko: '실패', ja: '失敗', de: "Fehlgeschlagen", fr: "Échec", it: "Non e' successo", 'es-ES': "Fallado", ru: "неудачник", pl: "Nie powiodło się", 'pt-BR': "Falhou" },
  // 说明
  details: { 'zh-CN': '说明', 'zh-TW': '說明', en: 'Details', ko: '설명', ja: '説明', de: "Einzelheiten", fr: "Détails", it: "Dettagli", 'es-ES': "Detalles", ru: "Подробности", pl: "Szczegóły", 'pt-BR': "Detalhes" },
  // 下载安装
  download: { 'zh-CN': '下载安装', 'zh-TW': '下載安裝', en: 'Download', ko: '다운로드 및 설치', ja: 'ダウンロードとインストール', de: "Downloaden", fr: "Télécharger", it: "Scarica", 'es-ES': "Descargar", ru: "скачать", pl: "Pobieranie", 'pt-BR': "Baixar" },
  // 先刷新源文档，再并行刷新各仓库
  reloadSourceDocumentsThenRefreshRepositoriesInParallel: { 'zh-CN': '先刷新源文档，再并行刷新各仓库', 'zh-TW': '先重新整理來源文件，再並行整理各倉庫', en: 'Reload source documents, then refresh repositories in parallel', ko: '소스 문서를 먼저 새로 고친 뒤 저장소를 병렬로 새로 고침', ja: 'ソース文書を更新してから各リポジトリを並列更新', de: "Quelldokumente neu laden, dann Repositories parallel aktualisieren", fr: "Recharger les documents sources, puis rafraîchir les dépôts en parallèle", it: "Caricare documenti sorgente, quindi aggiornare i repository in parallelo", 'es-ES': "Recargar los documentos fuente, luego refrescar los repositorios en paralelo", ru: "Перезагрузка исходных документов, затем обновление репозиториев параллельно", pl: "Odśwież dokumenty źródłowe, a następnie odświeżaj repozytoria równolegle", 'pt-BR': "Recarregar os documentos de origem e, em seguida, actualizar os repositórios em paralelo" },
  // 仓库源刷新失败，请稍后重试
  sourceRefreshFailedTryAgainLater: { 'zh-CN': '仓库源刷新失败，请稍后重试', 'zh-TW': '倉庫源重新整理失敗，請稍後重試', en: 'Source refresh failed. Try again later.', ko: '소스 새로 고침 실패, 잠시 후 다시 시도하세요', ja: 'ソースの更新に失敗しました。後で再試行してください', de: "Die Aktualisierung der Quelle ist fehlgeschlagen. Versuchen Sie es später noch einmal.", fr: "La mise à jour de la source a échoué.", it: "Il rinfresco della fonte è fallito.", 'es-ES': "El refresco de la fuente falló. Inténtalo de nuevo más tarde.", ru: "Обновление источника провалилось. Попробуй позже.", pl: "Odświeżenie źródła nie powiodło się. Spróbuj później.", 'pt-BR': "Falha na atualização do código-fonte. Tente novamente mais tarde." },
  // 许可证
  license: { 'zh-CN': '许可证', 'zh-TW': '授權條款', en: 'License', ko: '라이선스', ja: 'ライセンス', de: "Lizenz", fr: "Licence", it: "Licenza", 'es-ES': "Licencia", ru: "Лицензия", pl: "Licencja", 'pt-BR': "Licença" },
  // 最后更新
  updated: { 'zh-CN': '最后更新', 'zh-TW': '最後更新', en: 'Updated', ko: '최종 업데이트', ja: '最終更新', de: "Aktualisiert", fr: "Mise à jour", it: "Aggiornato", 'es-ES': "Actualizado", ru: "Обновлено", pl: "Aktualizacja", 'pt-BR': "Actualizado" },
  // 标识
  identifier: { 'zh-CN': '标识', 'zh-TW': '識別碼', en: 'Identifier', ko: '식별자', ja: '識別子', de: "Identifikator", fr: "Identificateur", it: "Identificazione", 'es-ES': "Identificador", ru: "Идентификатор", pl: "Identyfikator", 'pt-BR': "Identificador" },
  // 版本绑定
  versionBinding: { 'zh-CN': '版本绑定', 'zh-TW': '版本綁定', en: 'Version binding', ko: '버전 바인딩', ja: 'バージョン紐付け', de: "Version verbindlich", fr: "Reliure de version", it: "Rilegatura della versione", 'es-ES': "Versión vinculante", ru: "Версия обязательная", pl: "Wiązanie w wersji", 'pt-BR': "Encadernação da versão" },
  // 源码
  source: { 'zh-CN': '源码', 'zh-TW': '原始碼', en: 'Source', ko: '소스', ja: 'ソース', de: "Quelle", fr: "Sources", it: "Fonte", 'es-ES': "Fuente", ru: "Источник", pl: "Źródło", 'pt-BR': "Origem" },
  // Release
  release: { 'zh-CN': 'Release', 'zh-TW': 'Release', en: 'Release', ko: 'Release', ja: 'Release', de: "Release", fr: "Version publiée", it: "Versione rilasciata", 'es-ES': "Versión publicada", ru: "Релиз", pl: "Wydanie", 'pt-BR': "Versão publicada" },
  // 该仓库没有备注
  thisRepositoryHasNoNote: { 'zh-CN': '该仓库没有备注', 'zh-TW': '該倉庫沒有備註', en: 'This repository has no note', ko: '이 저장소에는 메모가 없습니다', ja: 'このリポジトリにはメモがありません', de: "Dieses Repository hat keine Notiz", fr: "Ce dépôt n'a aucune note", it: "Questo repository non ha alcuna nota", 'es-ES': "Este repositorio no tiene ninguna nota", ru: "Этот репозиторий не имеет заметки", pl: "To repozytorium nie ma notatki", 'pt-BR': "Este repositório não tem nenhuma nota" },
  // 正在加载仓库说明
  loadingRepositoryReadme: { 'zh-CN': '正在加载仓库说明', 'zh-TW': '正在載入倉庫說明', en: 'Loading repository README', ko: '저장소 README 불러오는 중', ja: 'リポジトリの説明を読み込み中', de: "Laderepository README", fr: "Chargement du dépôt README", it: "Repositore di caricamento README", 'es-ES': "Carga de repositorio README", ru: "Загрузочный репозиторий README", pl: "Wczytywanie repozytorium README", 'pt-BR': "Carregando repositório README" },
  // 没有匹配的仓库
  noMatchingRepositories: { 'zh-CN': '没有匹配的仓库', 'zh-TW': '沒有符合的倉庫', en: 'No matching repositories', ko: '일치하는 저장소가 없습니다', ja: '一致するリポジトリがありません', de: "Keine übereinstimmenden Datenspeicher", fr: "Pas de dépôts correspondants", it: "Nessun repository di corrispondenza", 'es-ES': "No hay repositorios coincidentes", ru: "Нет подходящих репозиториев", pl: "Brak pasujących repozytoriów", 'pt-BR': "Sem repositórios correspondentes" },
  // 部分启用
  partiallyEnabled: { 'zh-CN': '部分启用', 'zh-TW': '部分啟用', en: 'Partially enabled', ko: '일부 활성', ja: '一部有効', de: "Teilweise aktiviert", fr: "partiellement activé", it: "Abilitazione parziale", 'es-ES': "Posibilidad parcial", ru: "Частично разрешено", pl: "Częściowo włączone", 'pt-BR': "Ativado parcialmente" },
  // 可更新
  update: { 'zh-CN': '可更新', 'zh-TW': '可更新', en: 'Update', ko: '업데이트 가능', ja: '更新あり', de: "Aktualisieren", fr: "Mettre à jour", it: "Aggiorna", 'es-ES': "Actualizar", ru: "Обновить", pl: "Zaktualizuj", 'pt-BR': "Atualizar" },
  // 刷新失败
  refreshFailed: { 'zh-CN': '刷新失败', 'zh-TW': '重新整理失敗', en: 'Refresh failed', ko: '새로 고침 실패', ja: '更新失敗', de: "Refresh fehlgeschlagen", fr: "La restauration a échoué", it: "Rifiuti falliti", 'es-ES': "Fallo por el refresco", ru: "Обновление провалилось", pl: "Odśwież nieudany", 'pt-BR': "A actualização falhou" },
  // 已最新
  upToDate: { 'zh-CN': '已最新', 'zh-TW': '已最新', en: 'Up to date', ko: '최신 상태', ja: '最新です', de: "Aktualität", fr: "Mise à jour", it: "Fino ad oggi", 'es-ES': "Hasta la fecha", ru: "Вплоть до настоящего времени", pl: "Aktualny", 'pt-BR': "Actualizado" },
  // 低版本
  olderVersion: { 'zh-CN': '低版本', 'zh-TW': '低版本', en: 'Older version', ko: '이전 버전', ja: '旧バージョン', de: "Ältere Version", fr: "Ancienne version", it: "Versione più vecchia", 'es-ES': "Versión más antigua", ru: "Старая версия", pl: "Starsza wersja", 'pt-BR': "Versão mais antiga" },
  // 版本未知
  versionUnknown: { 'zh-CN': '版本未知', 'zh-TW': '版本未知', en: 'Version unknown', ko: '버전 미확인', ja: 'バージョン不明', de: "Version unbekannt", fr: "Version inconnue", it: "Versione sconosciuta", 'es-ES': "Versión desconocida", ru: "Версия неизвестна", pl: "Wersja nieznana", 'pt-BR': "Versão desconhecida" },
  // 检测中
  checking: { 'zh-CN': '检测中', 'zh-TW': '檢測中', en: 'Checking', ko: '확인 중', ja: '確認中', de: "Überprüfung", fr: "Vérification", it: "Controllo", 'es-ES': "Verificación", ru: "Проверка", pl: "Sprawdzanie", 'pt-BR': "Verificação" },
  // 刷新中（绑定仓库 / 地图源的 Release 正在重抓）
  refreshing: { 'zh-CN': '刷新中', 'zh-TW': '重新整理中', en: 'Refreshing', ko: '새로 고침 중', ja: '更新中', de: "Erfrischend", fr: "Rafraîchissement", it: "Raffreddamento", 'es-ES': "Refrescante", ru: "освежающий", pl: "Odświeżające", 'pt-BR': "Atualizando" },
  // 正在检测更新
  checkingForUpdates: { 'zh-CN': '正在检测更新', 'zh-TW': '正在檢查更新', en: 'Checking for updates', ko: '업데이트 확인 중', ja: '更新を確認中', de: "Überprüfung auf Updates", fr: "Vérification des mises à jour", it: "Controllo degli aggiornamenti", 'es-ES': "Comprobación de actualizaciones", ru: "Проверка обновлений", pl: "Sprawdzanie aktualizacji", 'pt-BR': "Verificando as atualizações" },
  // 排序
  sort: { 'zh-CN': '排序', 'zh-TW': '排序', en: 'Sort', ko: '정렬', ja: '並び替え', de: "Sortieren", fr: "Tri", it: "Ordinare", 'es-ES': "Más o menos", ru: "сортировать", pl: "Sortuj", 'pt-BR': "Ordenar" },
  // 准备中
  preparing: { 'zh-CN': '准备中', 'zh-TW': '準備中', en: 'Preparing', ko: '준비 중', ja: '準備中', de: "Vorbereitung", fr: "Préparation", it: "Preparazione", 'es-ES': "Preparación", ru: "Подготовка", pl: "Przygotowanie", 'pt-BR': "Preparação" },
  // 下载失败
  downloadFailed: { 'zh-CN': '下载失败', 'zh-TW': '下載失敗', en: 'Download failed', ko: '다운로드 실패', ja: 'ダウンロード失敗', de: "Download fehlgeschlagen", fr: "Le téléchargement a échoué", it: "Download non riuscito", 'es-ES': "Descarga fallido", ru: "Скачать Неудачный", pl: "Pobieranie zakończone niepowodzeniem", 'pt-BR': "A transferência falhou" },
  // 正在连接下载源…
  connectingToDownloadSource: { 'zh-CN': '正在连接下载源…', 'zh-TW': '正在連線下載來源…', en: 'Connecting to download source…', ko: '다운로드 소스에 연결 중…', ja: 'ダウンロード元に接続中…', de: "Verbindung zur Download-Quelle…", fr: "Connexion à la source de téléchargement…", it: "Connettersi a scaricare sorgente…", 'es-ES': "Conectarse a la fuente de descarga…", ru: "Подключение к источнику загрузки…", pl: "Łączenie ze źródłem pobierania…", 'pt-BR': "A ligar ao código de transferência…" },
  // 已完成
  completed: { 'zh-CN': '已完成', 'zh-TW': '已完成', en: 'Completed', ko: '완료', ja: '完了', de: "Abgeschlossen", fr: "Achevé", it: "Completo", 'es-ES': "Completado", ru: "Завершено", pl: "Zakończono", 'pt-BR': "Concluído" },
  // 选择版本
  selectVersion: { 'zh-CN': '选择版本', 'zh-TW': '選擇版本', en: 'Select version', ko: '버전 선택', ja: 'バージョンを選択', de: "Ausgewählte Fassung", fr: "Sélectionner la version", it: "Selezionare la versione", 'es-ES': "Seleccione la versión", ru: "Выберите версию", pl: "Wybierz wersję", 'pt-BR': "Seleccionar versão" },
  // 选择文件
  selectFile: { 'zh-CN': '选择文件', 'zh-TW': '選擇檔案', en: 'Select file', ko: '파일 선택', ja: 'ファイルを選択', de: "Select-Datei", fr: "Sélectionner un fichier", it: "Selezionare il file", 'es-ES': "Seleccione el archivo", ru: "Выберите файл", pl: "Wybierz plik", 'pt-BR': "Selecionar arquivo" },
  // 大小
  size: { 'zh-CN': '大小', 'zh-TW': '大小', en: 'Size', ko: '크기', ja: 'サイズ', de: "Größe", fr: "Taille", it: "Dimensione", 'es-ES': "Tamaño", ru: "Размер", pl: "Rozmiar", 'pt-BR': "Tamanho" },
  // 下载 {count} 次
  downloads: { 'zh-CN': '下载 {count} 次', 'zh-TW': '下載 {count} 次', en: '{count} downloads', ko: '{count}회 다운로드', ja: '{count} 回ダウンロード', de: "{count} downloaden", fr: "Téléchargements de {count}", it: "download di {count}", 'es-ES': "{count} descargas", ru: "{count} скачать", pl: "Pobieranie {count}", 'pt-BR': "Downloads do {count}" },
  // 另有 {count} 个匹配文件
  moreMatchingFiles: { 'zh-CN': '另有 {count} 个匹配文件', 'zh-TW': '另有 {count} 個符合檔案', en: '{count} more matching files', ko: '{count}개 추가 일치 파일', ja: '他に {count} 件の一致ファイル', de: "{count} mehr passende Dateien", fr: "{count} plus de fichiers correspondants", it: "{count} più file corrispondenti", 'es-ES': "{count} más archivos que coincidan", ru: "{count} больше похожих файлов", pl: "{count} więcej pasujących plików", 'pt-BR': "{count} mais ficheiros correspondentes" },
  // 该版本没有命中资产规则的文件
  noAssetInThisReleaseMatchesTheAssetRules: { 'zh-CN': '该版本没有命中资产规则的文件', 'zh-TW': '該版本沒有符合資產規則的檔案', en: 'No asset in this release matches the asset rules', ko: '이 버전에는 자산 규칙과 일치하는 파일이 없습니다', ja: 'このバージョンにアセット規則と一致するファイルがありません', de: "Kein Asset in diesem Release entspricht den Asset-Regeln", fr: "Aucun actif dans ce communiqué ne correspond aux règles de l'actif", it: "Nessuna risorsa in questa release corrisponde alle regole di asset", 'es-ES': "Ningún activo en esta liberación coincide con las reglas del activo", ru: "Ни один актив в этом выпуске не соответствует правилам актива", pl: "Żaden składnik aktywów w tym wydaniu nie spełnia zasad dotyczących aktywów", 'pt-BR': "Nenhum activo nesta versão corresponde às regras do activo" },
  // 仓库文件模式直接下载该文件
  repositoryFileModeDownloadsThisFileDirectly: { 'zh-CN': '仓库文件模式直接下载该文件', 'zh-TW': '倉庫檔案模式直接下載該檔案', en: 'Repository-file mode downloads this file directly', ko: '저장소 파일 모드는 이 파일을 바로 받습니다', ja: 'リポジトリファイルモードはこのファイルを直接ダウンロードします', de: "Repository-Datei-Modus lädt diese Datei direkt herunter", fr: "Le mode Dépôt-fichier télécharge ce fichier directement", it: "La modalità Repository-file scarica questo file direttamente", 'es-ES': "El modo Repository-file descarga este archivo directamente", ru: "Режим Repository-file загружает этот файл напрямую", pl: "Tryb reposity- file pobiera ten plik bezpośrednio", 'pt-BR': "O modo de arquivo de repositório baixa este arquivo diretamente" },
  // 当前渠道没有社区仓库
  noCommunityRepositoriesInThisChannel: { 'zh-CN': '当前渠道没有社区仓库', 'zh-TW': '目前渠道沒有社群倉庫', en: 'No community repositories in this channel', ko: '이 채널에는 커뮤니티 저장소가 없습니다', ja: 'このチャンネルにはコミュニティリポジトリがありません', de: "Keine Community-Repositorys in diesem Kanal", fr: "Aucun dépôt communautaire dans ce canal", it: "Nessun repository di comunità in questo canale", 'es-ES': "No hay repositorios comunitarios en este canal", ru: "Никаких репозиториев на этом канале", pl: "Brak repozytoriów społecznościowych w tym kanale", 'pt-BR': "Nenhum repositório comunitário neste canal" },
}
