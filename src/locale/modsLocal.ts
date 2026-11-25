import type { DomainMessages } from './types'

/**
 * modsLocal 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const modsLocal: DomainMessages = {
  installBeforeManaging: { 'zh-CN': '未安装 {name}，请安装后再来管理', 'zh-TW': '未安裝 {name}，請安裝後再來管理', en: '{name} is not installed. Install it before managing it here.', ko: '{name}이(가) 설치되지 않았습니다. 설치한 후 여기에서 관리하세요.', ja: '{name}がインストールされていません。インストールしてから管理してください。', de: "{name} ist nicht installiert. Installieren Sie es, bevor Sie es hier verwalten.", fr: "{name} n'est pas installé. Installez-le avant de le gérer ici.", it: "{name} non è installato. Installalo prima di gestirlo qui.", 'es-ES': "{name} no está instalado. Instalarlo antes de gestionarlo aquí.", ru: "{name} не установлен. Установите его, прежде чем управлять им здесь.", pl: "{name} nie jest zainstalowany. Zainstaluj go przed zarządzaniem nim tutaj.", 'pt-BR': "O {name} não está instalado. Instale- o antes de o gerir aqui." },
  // 名称
  name: { 'zh-CN': '名称', 'zh-TW': '名稱', en: 'Name', ko: '이름', ja: '名前', de: "Name des Unternehmens", fr: "Dénomination", it: "Nome", 'es-ES': "Nombre", ru: "Имя", pl: "Nazwa", 'pt-BR': "Nome" },
  // 卡片
  cards: { 'zh-CN': '卡片', 'zh-TW': '卡片', en: 'Cards', ko: '카드', ja: 'カード', de: "Karten", fr: "Cartes", it: "Carte", 'es-ES': "Tarjetas", ru: "Карты", pl: "Karty", 'pt-BR': "Cartas" },
  // 横排
  rows: { 'zh-CN': '横排', 'zh-TW': '橫排', en: 'Rows', ko: '행', ja: '行', de: "Reihen", fr: "Lignes", it: "Righe", 'es-ES': "Filas", ru: "ряды", pl: "Wiersze", 'pt-BR': "Linhas" },
  // 尚未安装任何 MOD
  noModsInstalled: { 'zh-CN': '尚未安装任何 MOD', 'zh-TW': '尚未安裝任何 MOD', en: 'No MODs installed', ko: '설치된 MOD 없음', ja: 'MODは未インストールです', de: "Keine MODs installiert", fr: "Pas de MODs installés", it: "Nessun MOD installato", 'es-ES': "No MODs instalados", ru: "Нет установленных MOD", pl: "Brak zainstalowanych MOD", 'pt-BR': "Nenhum MOD instalado" },
  // 没有匹配的 MOD
  noMatchMods: { 'zh-CN': '没有匹配的 MOD，换个关键词试试。', 'zh-TW': '沒有符合的 MOD，換個關鍵字試試。', en: 'No MODs match your search. Try a different keyword.', ko: '검색과 일치하는 MOD가 없습니다. 다른 키워드를 입력해 보세요.', ja: '検索に一致する MOD がありません。別のキーワードをお試しください。', de: "Keine MODs passen zu Ihrer Suche. Versuchen Sie ein anderes Keyword.", fr: "Aucun MOD ne correspond à votre recherche. Essayez un mot-clé différent.", it: "Nessun MODs corrispondono alla tua ricerca. Prova una parola chiave diversa.", 'es-ES': "No hay MODs que coincidan con su búsqueda. Pruebe una palabra clave diferente.", ru: "Никаких шаблонов поиска. Попробуйте другое ключевое слово.", pl: "Brak MOD pasuje do wyszukiwania. Spróbuj innego słowa kluczowego.", 'pt-BR': "Nenhum MOD corresponde à sua pesquisa. Tente uma palavra- chave diferente." },
  // 有更新
  update: { 'zh-CN': '有更新', 'zh-TW': '有更新', en: 'Update', ko: '업데이트', ja: '更新あり', de: "Aktualisieren", fr: "Mettre à jour", it: "Aggiorna", 'es-ES': "Actualizar", ru: "Обновить", pl: "Zaktualizuj", 'pt-BR': "Atualizar" },
  // 最新版
  latest: { 'zh-CN': '最新版', 'zh-TW': '最新版', en: 'Latest', ko: '최신', ja: '最新', de: "Letztes", fr: "Dernière", it: "Più recenti", 'es-ES': "Últimas", ru: "последний", pl: "Najnowsze", 'pt-BR': "Última" },
  // 详情
  details: { 'zh-CN': '详情', 'zh-TW': '詳情', en: 'Details', ko: '상세', ja: '詳細', de: "Einzelheiten", fr: "Détails", it: "Dettagli", 'es-ES': "Detalles", ru: "Подробности", pl: "Szczegóły", 'pt-BR': "Detalhes" },
  // 收起
  collapse: { 'zh-CN': '收起', 'zh-TW': '收起', en: 'Collapse', ko: '접기', ja: '折りたたむ', de: "Kollaps", fr: "Effondré", it: "Colpisco", 'es-ES': "Colapso", ru: "обрушение", pl: "Zwiń", 'pt-BR': "Recolher" },
  // 无更新
  upToDate: { 'zh-CN': '无更新', 'zh-TW': '無更新', en: 'Up to date', ko: '최신 상태', ja: '最新です', de: "Aktualität", fr: "Mise à jour", it: "Fino ad oggi", 'es-ES': "Hasta la fecha", ru: "Вплоть до настоящего времени", pl: "Aktualny", 'pt-BR': "Actualizado" },
  // 未绑定
  unbound: { 'zh-CN': '未绑定', 'zh-TW': '未綁定', en: 'Unbound', ko: '미연결', ja: '未リンク', de: "Ungebunden", fr: "Non consolidé", it: "Non c'è più", 'es-ES': "Sin límites", ru: "несвязанный", pl: "Brak ograniczeń", 'pt-BR': "Não consolidado" },
  unlisted: { 'zh-CN': '未收录', 'zh-TW': '未收錄', en: 'Not listed', ko: '목록에 없음', ja: '未掲載', de: "Nicht aufgeführt", fr: "Non listé", it: "Non elencato", 'es-ES': "No incluido", ru: "Не перечислено", pl: "Niewymienione", 'pt-BR': "Não listado" },
  sourceUnconfirmed: { 'zh-CN': '来源未确认', 'zh-TW': '來源未確認', en: 'Source unconfirmed', ko: '출처 미확인', ja: '配布元未確認', de: "Quelle nicht bestätigt", fr: "Source non confirmée", it: "Fonte non confermata", 'es-ES': "Fuente no confirmada", ru: "Источник неподтвержденный", pl: "Źródło niepotwierdzone", 'pt-BR': "Fonte não confirmada" },
  // 仓库缺失
  repositoryMissing: { 'zh-CN': '仓库缺失', 'zh-TW': '倉庫缺失', en: 'ModRepository missing', ko: '저장소 없음', ja: 'リポジトリなし', de: "ModRepository fehlt", fr: "ModRepository manquant", it: "ModRepository mancante", 'es-ES': "ModRepository desaparecido", ru: "Отсутствующий репозиторий", pl: "Brak modułu Repozytorium", 'pt-BR': "Falta o ModRepository" },
  // 重复
  duplicate: { 'zh-CN': '重复', 'zh-TW': '重複', en: 'Duplicate', ko: '중복', ja: '重複', de: "Duplikat", fr: "Dupliquer", it: "Duplicare", 'es-ES': "Duplicar", ru: "дублировать", pl: "Powielanie", 'pt-BR': "Duplicar" },
  // 旧副本
  oldCopy: { 'zh-CN': '旧副本', 'zh-TW': '舊副本', en: 'Old copy', ko: '이전 사본', ja: '古いコピー', de: "Alte Kopie", fr: "Ancien exemplaire", it: "Vecchia copia", 'es-ES': "Copia antigua", ru: "Старая копия", pl: "Stary egzemplarz", 'pt-BR': "Cópia antiga" },
  // 加载警告
  loadWarning: { 'zh-CN': '加载警告', 'zh-TW': '載入警告', en: 'Load warning', ko: '로드 경고', ja: '読み込み警告', de: "Lastwarnung", fr: "Avertissement de charge", it: "Avvertenza del carico", 'es-ES': "Alerta de carga", ru: "Предупреждение о грузе", pl: "Ostrzeżenie o obciążeniu", 'pt-BR': "Aviso de carga" },
  // 已禁用
  disabled: { 'zh-CN': '已禁用', 'zh-TW': '已停用', en: 'Disabled', ko: '비활성', ja: '無効', de: "Deaktiviert", fr: "Désactivé", it: "Disabilitato", 'es-ES': "Desactivado", ru: "Отключено", pl: "Wyłączono", 'pt-BR': "Desativado" },
  // GUID：{guid}
  guid: { 'zh-CN': 'GUID：{guid}', 'zh-TW': 'GUID：{guid}', en: 'GUID: {guid}', ko: 'GUID: {guid}', ja: 'GUID: {guid}', de: "GUID: {guid}", fr: "GUID: {guid}", it: "GUID: {guid}", 'es-ES': "GUID: {guid}", ru: "GUID: {guid}", pl: "GUID: {guid}", 'pt-BR': "GUID: {guid}" },
  // 文件：{file}
  file: { 'zh-CN': '文件：{file}', 'zh-TW': '檔案：{file}', en: 'File: {file}', ko: '파일: {file}', ja: 'ファイル: {file}', de: "Datei: {file}", fr: "Dossier : {file}", it: "Archivio: {file}", 'es-ES': "Archivo: {file}", ru: "Источник: {file}", pl: "Plik: {file}", 'pt-BR': "Arquivo: {file}" },
  // MOD 包：{name}（{count} 个）
  modPackage: { 'zh-CN': 'MOD 包：{name}（{count} 个）', 'zh-TW': 'MOD 包：{name}（{count} 個）', en: 'MOD package: {name} ({count})', ko: 'MOD 패키지: {name}({count}개)', ja: 'MOD パッケージ: {name}（{count} 件）', de: "MOD-Paket: {name} ({count})", fr: "MODÈLE: {name} ({count})", it: "Pacchetto MOD: {name} ({count})", 'es-ES': "Paquete MOD: {name} ({count})", ru: "Мод-пакет: {name} ({count})", pl: "Pakiet MOD: {name} ({count})", 'pt-BR': "Pacote MOD: {name} ({count})" },
  // 警告：{warning}
  warning: { 'zh-CN': '警告：{warning}', 'zh-TW': '警告：{warning}', en: 'Warning: {warning}', ko: '경고: {warning}', ja: '警告: {warning}', de: "Warnung: {warning}", fr: "Avertissement : {warning}", it: "Attenzione: {warning}", 'es-ES': "Advertencia: {warning}", ru: "Предупреждение: {warning}", pl: "Ostrzeżenie: {warning}", 'pt-BR': "Aviso: {warning}" },
  // 删除 MOD
  deleteMod: { 'zh-CN': '删除 MOD', 'zh-TW': '刪除 MOD', en: 'Delete MOD', ko: 'MOD 삭제', ja: 'MOD を削除', de: "MOD löschen", fr: "Supprimer le MOD", it: "Eliminare il MOD", 'es-ES': "Eliminar el MOD", ru: "Удалить MOD", pl: "Usuń MOD", 'pt-BR': "Apagar MOD" },
  uninstallOlderCopy: { 'zh-CN': '卸载旧版本', 'zh-TW': '解除安裝舊版本', en: 'Uninstall older copy', ko: '이전 버전 제거', ja: '旧バージョンをアンインストール', de: "Ältere Kopie deinstallieren", fr: "Désinstaller la copie ancienne", it: "Disinstallare la copia precedente", 'es-ES': "Desinstalar copia anterior", ru: "Удалить старую копию", pl: "Odinstaluj starszą kopię", 'pt-BR': "Desinstalar cópia antiga" },
  keepNewerCopy: { 'zh-CN': '已启用的较新版本会保留。', 'zh-TW': '已啟用的較新版本會保留。', en: 'The newer enabled copy will be kept.', ko: '활성화된 최신 버전은 유지됩니다.', ja: '有効な新しいバージョンは保持されます。', de: "Die neuere aktivierte Kopie wird beibehalten.", fr: "La copie nouvellement activée sera conservée.", it: "La nuova copia abilitata verrà mantenuta.", 'es-ES': "Se mantendrá la copia habilitada más reciente.", ru: "Новая копия будет сохранена.", pl: "Nowsza włączona kopia zostanie zachowana.", 'pt-BR': "A cópia ativa mais recente será mantida." },
  // 「{name}」将被移入回收站，可在回收站中恢复。继续？
  willBeMovedToTheRecycleBinAndCanBeRestoredContinue: { 'zh-CN': '「{name}」将被移入回收站，可在回收站中恢复。继续？', 'zh-TW': '「{name}」將被移入資源回收筒，可在資源回收筒中還原。繼續？', en: '"{name}" will be moved to the recycle bin and can be restored. Continue?', ko: '"{name}"을(를) 휴지통으로 옮깁니다. 복원할 수 있습니다. 계속할까요?', ja: '「{name}」をゴミ箱へ移動します（復元可能）。続行しますか？', de: "\"{name}\" wird in den Papierkorb gebracht und kann wiederhergestellt werden.", fr: "\"{name}\" sera déplacé dans le bac de recyclage et peut être restauré.", it: "\"{name}\" sarà spostato nel cestino e può essere ripristinato.", 'es-ES': "\"{name}\" será trasladado a la papelera de reciclaje y puede ser restaurado. ¿Continúe?", ru: "\"{name}\" будет перемещен в мусорное ведро и может быть восстановлено.", pl: "\"{name}\" zostanie przeniesiony do kosza na recykling i może zostać przywrócony. Kontynuować?", 'pt-BR': "\"{name}\" será movido para a lixeira e pode ser restaurado. Continuar?" },
  // 移入回收站
  moveToRecycleBin: { 'zh-CN': '移入回收站', 'zh-TW': '移入資源回收筒', en: 'Move to recycle bin', ko: '휴지통으로 이동', ja: 'ゴミ箱へ移動', de: "Umzug in den Papierkorb", fr: "Déplacer vers le bac de recyclage", it: "Spostare il cestino", 'es-ES': "Muévete a la papelera de reciclaje", ru: "Перейти на переработку мусора", pl: "Przenieś do recyklingu kosza", 'pt-BR': "Mover para a lixeira" },
  // 「{name}」将被直接删除，无法恢复。继续？
  willBeDeletedPermanentlyThisCannotBeUndoneContinue: { 'zh-CN': '「{name}」将被直接删除，无法恢复。继续？', 'zh-TW': '「{name}」將被直接刪除，無法還原。繼續？', en: '"{name}" will be deleted permanently. This cannot be undone. Continue?', ko: '"{name}"을(를) 바로 삭제합니다. 복원할 수 없습니다. 계속할까요?', ja: '「{name}」を直接削除します。元に戻せません。続行しますか？', de: "„{name}“ wird endgültig gelöscht. Das lässt sich nicht rückgängig machen. Fortfahren ?", fr: "« {name} » sera supprimé définitivement. Cette action est irréversible. Continuer ?", it: "«{name}» verrà eliminato definitivamente. L’operazione è irreversibile. Continuare?", 'es-ES': "«{name}» se eliminará de forma permanente. No se puede deshacer. ¿Continuar?", ru: "«{name}» будет удалён навсегда. Это действие нельзя отменить. Продолжить?", pl: "„{name}” zostanie trwale usunięty. Tej operacji nie można cofnąć. Kontynuować?", 'pt-BR': "“{name}” será excluído permanentemente. Não é possível desfazer esta ação. Continuar?" },
  // 直接删除
  deletePermanently: { 'zh-CN': '直接删除', 'zh-TW': '直接刪除', en: 'Delete permanently', ko: '바로 삭제', ja: '直接削除', de: "Dauerhaft löschen", fr: "Supprimer définitivement", it: "Eliminare permanentemente", 'es-ES': "Suprímase permanentemente", ru: "Удалить навсегда", pl: "Usuń na stałe", 'pt-BR': "Apagar permanentemente" },
  // 该 MOD 属于 MOD 包
  thisModBelongsToAPackage: { 'zh-CN': '该 MOD 属于 MOD 包', 'zh-TW': '該 MOD 屬於 MOD 包', en: 'This MOD belongs to a package', ko: '이 MOD는 패키지에 속합니다', ja: 'このMODはパッケージに属します', de: "Dieser MOD gehört zu einem Paket", fr: "Ce MOD appartient à un paquet", it: "Questo MOD appartiene a un pacchetto", 'es-ES': "Este MOD pertenece a un paquete", ru: "Данная модель относится к пакету", pl: "Ten MOD należy do pakietu", 'pt-BR': "Este MOD pertence a um pacote" },
  // 它与 {count} 个子 MOD 同属一个包，一并删除？
  itSharesAPackageWithModsDeleteThemTogether: { 'zh-CN': '它与 {count} 个子 MOD 同属一个包，一并删除？', 'zh-TW': '它與 {count} 個子 MOD 同屬一個包，一併刪除？', en: 'It shares a package with {count} MODs. Delete them together?', ko: '{count}개 MOD와 같은 패키지입니다. 함께 삭제할까요?', ja: '{count} 件のMODと同じパッケージです。まとめて削除しますか？', de: "Es teilt sich ein Paket mit {count} MODs. Löschen Sie sie zusammen?", fr: "Il partage un paquet avec {count} MODs. Les supprimer ensemble?", it: "Condivide un pacchetto con {count} MODs. Eliminarli insieme?", 'es-ES': "Comparte un paquete con los MOD {count}. ¿Desecharlos juntos?", ru: "Он делится пакетом с {count}. Удалить их вместе?", pl: "Udostępnia pakiet {count} MODs. Usuń je razem?", 'pt-BR': "Ele compartilha um pacote com os MODs {count}. Excluí- los juntos?" },
  // 整包删除
  deletePackage: { 'zh-CN': '整包删除', 'zh-TW': '整包刪除', en: 'Delete package', ko: '패키지 삭제', ja: 'パッケージを削除', de: "Paket löschen", fr: "Supprimer le paquet", it: "Elimina il pacchetto", 'es-ES': "Borrar el paquete", ru: "Удалить пакет", pl: "Usuń pakiet", 'pt-BR': "Apagar o pacote" },
  // 整包开关会同时切换包内 {count} 个子 MOD
  packageSwitchHint: { 'zh-CN': '整包开关会同时切换包内 {count} 个子 MOD', 'zh-TW': '整包開關會同時切換包內 {count} 個子 MOD', en: 'Toggling the package switches all {count} MODs inside it', ko: '패키지 스위치는 패키지 내 MOD {count}개를 함께 전환합니다', ja: 'パッケージのスイッチはパッケージ内の {count} 個の MOD をまとめて切り替えます', de: "Umschalten des Pakets schaltet alle {count} MODs darin", fr: "Le paquet bascule tous les modèles {count} à l'intérieur", it: "Toggling del pacchetto interruttori tutti {count} MODs dentro di esso", 'es-ES': "Toggling el paquete cambia todos los MOD {count} dentro de él", ru: "Переключение пакета переключает все модули {count} внутри него", pl: "Przełączanie pakietów włącza wszystkie {count} MOD wewnątrz pakietu", 'pt-BR': "Alternando o pacote muda todos os MODs {count} dentro dele" },
  // 已启用包内 {count} 个子 MOD
  packageEnabledToast: { 'zh-CN': '已启用包内 {count} 个子 MOD', 'zh-TW': '已啟用包內 {count} 個子 MOD', en: 'Enabled all {count} MODs in the package', ko: '패키지 내 MOD {count}개를 활성화했습니다', ja: 'パッケージ内の {count} 個の MOD を有効にしました', de: "Aktiviert alle {count} MODs im Paket", fr: "Activé tous les modèles {count} dans le paquet", it: "Abilitato tutti i MOD {count} nel pacchetto", 'es-ES': "Habilitó todos los MOD {count} en el paquete", ru: "Включить все {count} в пакет", pl: "Włączenie wszystkich MOD {count} w pakiecie", 'pt-BR': "Habilitou todos os MODs {count} no pacote" },
  // 已禁用包内 {count} 个子 MOD
  packageDisabledToast: { 'zh-CN': '已禁用包内 {count} 个子 MOD', 'zh-TW': '已停用包內 {count} 個子 MOD', en: 'Disabled all {count} MODs in the package', ko: '패키지 내 MOD {count}개를 비활성화했습니다', ja: 'パッケージ内の {count} 個の MOD を無効にしました', de: "Deaktivieren Sie alle {count} MODs im Paket", fr: "Désactivé tous les modèles {count} dans le paquet", it: "Disabilita tutti i MOD {count} nel pacchetto", 'es-ES': "Desactivar todos los MOD {count} en el paquete", ru: "Отключил все {count} MODs в упаковке", pl: "Wyłączył wszystkie MOD {count} w opakowaniu", 'pt-BR': "Desactivar todos os MODs {count} no pacote" },
  // 切换布局（卡片 / 横排）
  switchLayoutCardsRows: { 'zh-CN': '切换布局（卡片 / 横排）', 'zh-TW': '切換佈局（卡片 / 橫排）', en: 'Switch layout (cards / rows)', ko: '레이아웃 전환(카드/행)', ja: 'レイアウト切替（カード／行）', de: "Switch-Layout (Karten / Zeilen)", fr: "Mise en page du commutateur (cartes/lignes)", it: "Interruttore di layout (carte / righe)", 'es-ES': "Diseño de conmutación (tarjetas / filas)", ru: "Коммутационный макет (карты/ряды)", pl: "Układ przełączania (karty / wiersze)", 'pt-BR': "Mudar de disposição (cartas / linhas)" },
  // 排序：{sort}
  sort: { 'zh-CN': '排序：{sort}', 'zh-TW': '排序：{sort}', en: 'Sort: {sort}', ko: '정렬: {sort}', ja: '並び: {sort}', de: "Sortierung: {sort}", fr: "Classer : {sort}", it: "Tipo: {sort}", 'es-ES': "Ordenar: {sort}", ru: "Исполнитель: {sort}", pl: "Sortuj: {sort}", 'pt-BR': "Ordenar: {sort}" },
  // 全部刷新（逐仓独立，互不阻塞）
  refreshAllPerRepositoryNonBlocking: { 'zh-CN': '全部刷新（逐仓独立，互不阻塞）', 'zh-TW': '全部重新整理（逐倉獨立，互不阻塞）', en: 'Refresh all (per repository, non-blocking)', ko: '모두 새로 고침(저장소별 독립)', ja: 'すべて更新（リポジトリ単位・非ブロッキング）', de: "Refresh all (pro Repository, nicht blockierend)", fr: "Tout rafraîchir (par dépôt, sans blocage)", it: "Rifiutare tutto (per repository, non-blocking)", 'es-ES': "Refresca todo (por repositorio, no bloqueo)", ru: "Обновить все (в репозитории, неблокируя)", pl: "Odśwież wszystkie (dla każdego repozytorium, nieblokujące)", 'pt-BR': "Atualizar tudo (por repositório, sem bloqueio)" },
  // 打开所在文件夹并定位
  revealInFolder: { 'zh-CN': '打开所在文件夹并定位', 'zh-TW': '開啟所在資料夾並定位', en: 'Reveal in folder', ko: '폴더에서 위치 열기', ja: 'フォルダーで表示', de: "Enthüllung im Ordner", fr: "Révèle dans le dossier", it: "Rivelare nella cartella", 'es-ES': "Revelar en carpeta", ru: "Показать в папке", pl: "Ujawnij w folderze", 'pt-BR': "Revelar na pasta" },
  // 查看日志
  viewLogs: { 'zh-CN': '查看日志', 'zh-TW': '檢視日誌', en: 'View logs', ko: '로그 보기', ja: 'ログを表示', de: "Sichtprotokolle", fr: "Afficher les journaux", it: "Visualizza i log", 'es-ES': "Ver registros", ru: "Посмотреть журналы", pl: "Przeglądanie dzienników", 'pt-BR': "Ver os registos" },
  // 打开目录
  openFolder: { 'zh-CN': '打开目录', 'zh-TW': '開啟目錄', en: 'Open folder', ko: '폴더 열기', ja: 'フォルダーを開く', de: "Ordner öffnen", fr: "Ouvrir le dossier", it: "Apri cartella", 'es-ES': "Abrir carpeta", ru: "Открыть папку", pl: "Otwórz folder", 'pt-BR': "Abrir pasta" },
  // 打开
  open: { 'zh-CN': '打开', 'zh-TW': '開啟', en: 'Open', ko: '열기', ja: '開く', de: "Öffnen", fr: "Ouvrir", it: "Apri", 'es-ES': "Abrir", ru: "Открыть", pl: "Otwórz", 'pt-BR': "Abrir" },
  // 标题
  title: { 'zh-CN': '标题', 'zh-TW': '標題', en: 'Title', ko: '제목', ja: 'タイトル', de: "Titel", fr: "Titre", it: "Titolo", 'es-ES': "Título", ru: "Название", pl: "Tytuł", 'pt-BR': "Título" },
  // 产品名
  productName: { 'zh-CN': '产品名', 'zh-TW': '產品名', en: 'Product name', ko: '제품명', ja: '製品名', de: "Produktbezeichnung", fr: "Nom du produit", it: "Nome del prodotto", 'es-ES': "Nombre del producto", ru: "Название продукта", pl: "Nazwa produktu", 'pt-BR': "Denominação do produto" },
  // 原始文件名
  originalFilename: { 'zh-CN': '原始文件名', 'zh-TW': '原始檔名', en: 'Original filename', ko: '원본 파일명', ja: '元のファイル名', de: "Ursprünglicher Dateiname", fr: "Nom du fichier original", it: "Nome del file originale", 'es-ES': "Nombre de archivo original", ru: "Оригинальное название: Filename", pl: "Oryginalna nazwa pliku", 'pt-BR': "Nome do arquivo original" },
  // 文件
  file2: { 'zh-CN': '文件', 'zh-TW': '檔案', en: 'File', ko: '파일', ja: 'ファイル', de: "Datei", fr: "Fichier", it: "File", 'es-ES': "Archivo", ru: "Файл", pl: "Plik", 'pt-BR': "Arquivo" },
  // 插件名
  pluginName: { 'zh-CN': '插件名', 'zh-TW': '外掛名', en: 'Plugin name', ko: '플러그인 이름', ja: 'プラグイン名', de: "Plugin-Namen", fr: "Nom du greffon", it: "Nome del plugin", 'es-ES': "Nombre de plugin", ru: "Имя плагина", pl: "Nazwa wtyczki", 'pt-BR': "Nome do plug-in" },
  // 角色（后端 `DllRole`：普通插件 / 依赖）
  role: { 'zh-CN': '角色', 'zh-TW': '角色', en: 'Role', ko: '역할', ja: '役割', de: "Rolle", fr: "Rôle", it: "Ruolo", 'es-ES': "Función", ru: "Роль", pl: "Rola", 'pt-BR': "Papel" },
  // 卡片标题后的灰色标签：这个 DLL 是包里的依赖库，不是独立 MOD
  bundledDependency: { 'zh-CN': '随包依赖', 'zh-TW': '隨包依賴', en: 'Bundled dependency', ko: '동봉 의존성', ja: '同梱依存', de: "Gebündelte Abhängigkeit", fr: "Dépendance groupée", it: "Dipendenza ridotta", 'es-ES': "Dependencia de aldeas", ru: "Связанная зависимость", pl: "Zależność wiązana", 'pt-BR': "Dependência agrupada" },
  mapBundledPlugin: { 'zh-CN': '地图插件', 'zh-TW': '地圖外掛', en: 'Map plugin', ko: '맵 플러그인', ja: 'マッププラグイン', de: "Plug-in Map", fr: "Greffon Map", it: "Mappa plugin", 'es-ES': "Mapa plugin", ru: "Плагин карты", pl: "Wtyczka mapy", 'pt-BR': "Plug- in de mapas" },
  mapBundledDependency: { 'zh-CN': '地图依赖', 'zh-TW': '地圖依賴', en: 'Map dependency', ko: '맵 의존성', ja: 'マップ依存', de: "Kartenabhängigkeit", fr: "Dépendance de la carte", it: "Dipendenza della mappa", 'es-ES': "Dependencia de mapas", ru: "Карта зависимости", pl: "Zależność od mapy", 'pt-BR': "Dependência do mapa" },
  mapBundledDlls: { 'zh-CN': '地图随包 DLL', 'zh-TW': '地圖隨包 DLL', en: 'DLLs bundled with maps', ko: '맵에 포함된 DLL', ja: 'マップ同梱 DLL', de: "DLLs gebündelt mit Karten", fr: "DLLs livré avec des cartes", it: "DLLs in bundle con le mappe", 'es-ES': "DLLs con mapas", ru: "DLLs в комплекте с картами", pl: "DLLs połączone z mapami", 'pt-BR': "DLLs com mapas" },
  // 作者
  author: { 'zh-CN': '作者', 'zh-TW': '作者', en: 'Author', ko: '제작자', ja: '作者', de: "Autor", fr: "Auteur", it: "Autore", 'es-ES': "Autor", ru: "Автор", pl: "Autor", 'pt-BR': "Autor" },
  // 版权
  copyright: { 'zh-CN': '版权', 'zh-TW': '版權', en: 'Copyright', ko: '저작권', ja: '著作権', de: "Urheberrecht", fr: "Droit d'auteur", it: "Diritto d'autore", 'es-ES': "Derechos", ru: "Авторское право", pl: "Prawa autorskie", 'pt-BR': "Direitos autorais" },
  // 版本来源
  versionSource: { 'zh-CN': '版本来源', 'zh-TW': '版本來源', en: 'Version source', ko: '버전 출처', ja: 'バージョン取得元', de: "Versionsquelle", fr: "Source de version", it: "Fonte di versione", 'es-ES': "Fuente de la versión", ru: "Источник версии", pl: "Źródło wersji", 'pt-BR': "Fonte da versão" },
  // 启用状态
  enabled: { 'zh-CN': '启用状态', 'zh-TW': '啟用狀態', en: 'Enabled', ko: '활성 상태', ja: '有効状態', de: "Aktiviert", fr: "Activé", it: "Abilitato", 'es-ES': "Activado", ru: "Включено", pl: "Włączono", 'pt-BR': "Ativado" },
  // 配置文件
  configFile: { 'zh-CN': '配置文件', 'zh-TW': '設定檔', en: 'Config file', ko: '설정 파일', ja: '設定ファイル', de: "Konfigurierungsdatei", fr: "Configurer le fichier", it: "File di configurazione", 'es-ES': "Archivo de Config", ru: "Файл конфигурирования", pl: "Plik konfiguracyjny", 'pt-BR': "Arquivo de configuração" },
  // 重复 GUID
  duplicateGuid: { 'zh-CN': '重复 GUID', 'zh-TW': '重複 GUID', en: 'Duplicate GUID', ko: '중복 GUID', ja: '重複 GUID', de: "Duplikat GUID", fr: "Dupliquer GUID", it: "Duplicare GUID", 'es-ES': "Duplicar GUID", ru: "Дублированный GUID", pl: "Duplikat GUID", 'pt-BR': "Duplicar GUID" },
  // 已启用
  enabled2: { 'zh-CN': '已启用', 'zh-TW': '已啟用', en: 'Enabled', ko: '활성', ja: '有効', de: "Aktiviert", fr: "Activé", it: "Abilitato", 'es-ES': "Activado", ru: "Включено", pl: "Włączono", 'pt-BR': "Ativado" },
  // 已安装
  installed: { 'zh-CN': '已安装', 'zh-TW': '已安裝', en: 'Installed', ko: '설치됨', ja: 'インストール済み', de: "Installiert", fr: "Installé", it: "Installato", 'es-ES': "Instalado", ru: "Установлено", pl: "Zainstalowano", 'pt-BR': "Instalado" },
  // 有
  yes: { 'zh-CN': '有', 'zh-TW': '有', en: 'Yes', ko: '있음', ja: 'あり', de: "Ja", fr: "Oui", it: "Sì", 'es-ES': "Sí", ru: "Да", pl: "Tak", 'pt-BR': "Sim" },
  // 无
  no: { 'zh-CN': '无', 'zh-TW': '無', en: 'No', ko: '없음', ja: 'なし', de: "Nein", fr: "Non", it: "No", 'es-ES': "No", ru: "Нет", pl: "Nie", 'pt-BR': "Não" },
  // 是
  yes2: { 'zh-CN': '是', 'zh-TW': '是', en: 'Yes', ko: '예', ja: 'はい', de: "Ja", fr: "Oui", it: "Sì", 'es-ES': "Sí", ru: "Да", pl: "Tak", 'pt-BR': "Sim" },
  // 否
  no2: { 'zh-CN': '否', 'zh-TW': '否', en: 'No', ko: '아니요', ja: 'いいえ', de: "Nein", fr: "Non", it: "No", 'es-ES': "No", ru: "Нет", pl: "Nie", 'pt-BR': "Não" },
  // 状态标签
  statusLabels: { 'zh-CN': '状态标签', 'zh-TW': '狀態標籤', en: 'Status labels', ko: '상태 라벨', ja: 'ステータスラベル', de: "Statuskennzeichen", fr: "Étiquettes de statut", it: "Etichette di stato", 'es-ES': "Etiquetas del estado", ru: "Ярлыки статуса", pl: "Etykiety statusu", 'pt-BR': "Rótulos de status" },
  // 提交日志
  releaseNotes: { 'zh-CN': '提交日志', 'zh-TW': '提交日誌', en: 'Release notes', ko: '릴리스 노트', ja: 'リリースノート', de: "Veröffentlichungsnotizen", fr: "Notes de mise à jour", it: "Note di rilascio", 'es-ES': "Notas de lanzamiento", ru: "Записи с релизами", pl: "Uwagi do wydania", 'pt-BR': "Notas de versão" },
  // 预发布版本
  preReleaseVersion: { 'zh-CN': '预发布版本', 'zh-TW': '預發布版本', en: 'Pre-release version', ko: '프리릴리스 버전', ja: 'プレリリース版', de: "Vorabversion", fr: "Version préalable à la publication", it: "Versione pre-release", 'es-ES': "Versión de publicación previa", ru: "Предварительная версия", pl: "Wersja prerelease", 'pt-BR': "Versão pré-lançamento" },
  // 此版本为预览版本，可能存在未知问题，是否继续安装？
  thisIsAPreReleaseBuildAndMayHaveUnknownIssuesContinueInstalling: { 'zh-CN': '此版本为预览版本，可能存在未知问题，是否继续安装？', 'zh-TW': '此版本為預覽版本，可能存在未知問題，是否繼續安裝？', en: 'This is a pre-release build and may have unknown issues. Continue installing?', ko: '이 버전은 프리릴리스이며 알려지지 않은 문제가 있을 수 있습니다. 계속 설치할까요?', ja: 'このバージョンはプレリリースのため、未知の問題が含まれる可能性があります。インストールを続行しますか？', de: "Dies ist ein Pre-Release-Build und kann unbekannte Probleme haben.", fr: "Il s'agit d'une compilation pré-release et peut avoir des problèmes inconnus.", it: "Questa è una costruzione pre-release e può avere problemi sconosciuti.", 'es-ES': "Esta es una construcción pre-release y puede tener problemas desconocidos. ¿Continúe instalando?", ru: "Это сборка перед выпуском и может иметь неизвестные проблемы.", pl: "Jest to prerelease build i może mieć nieznane problemy. Kontynuować instalację?", 'pt-BR': "Esta é uma compilação pré-lançamento e pode ter problemas desconhecidos. Continuar a instalar?" },
  // 继续安装
  continueInstalling: { 'zh-CN': '继续安装', 'zh-TW': '繼續安裝', en: 'Continue installing', ko: '계속 설치', ja: 'インストールを続行', de: "Weitere Installation", fr: "Continuer l'installation", it: "Continua a installare", 'es-ES': "Continuar instalando", ru: "Продолжить установку", pl: "Kontynuuj instalację", 'pt-BR': "Continuar a instalar" },
  // 该版本没有提交日志
  thisReleaseHasNoNotes: { 'zh-CN': '该版本没有提交日志', 'zh-TW': '該版本沒有提交日誌', en: 'This release has no notes', ko: '이 버전에는 릴리스 노트가 없습니다', ja: 'このバージョンにはリリースノートがありません', de: "Dieses Release hat keine Notizen", fr: "Cette version n'a pas de notes", it: "Questo rilascio non ha note", 'es-ES': "Esta liberación no tiene notas", ru: "В этом выпуске нет заметок", pl: "To wydanie nie ma notatek", 'pt-BR': "Esta versão não tem notas" },
  // 绑定仓库
  boundRepository: { 'zh-CN': '绑定仓库', 'zh-TW': '綁定倉庫', en: 'Bound repository', ko: '연결된 저장소', ja: '紐付けリポジトリ', de: "Gebundenes Repository", fr: "Dépôt solide", it: "repository Bound", 'es-ES': "Repositorio de libras", ru: "Связанный репозиторий", pl: "Repozytorium wiązane", 'pt-BR': "Repositório delimitado" },
  // 删除
  delete: { 'zh-CN': '删除', 'zh-TW': '刪除', en: 'Delete', ko: '삭제', ja: '削除', de: "Löschen", fr: "Supprimer", it: "Elimina", 'es-ES': "Eliminar", ru: "Удалить", pl: "Usuń", 'pt-BR': "Excluir" },
  // 已移入回收站
  movedToRecycleBin: { 'zh-CN': '已移入回收站', 'zh-TW': '已移入資源回收筒', en: 'Moved to the recycle bin', ko: '휴지통으로 옮겼습니다', ja: 'ゴミ箱へ移動しました', de: "Umzug in den Recycling-Binn", fr: "Déménagement à la poubelle de recyclage", it: "Spostato nel cestino", 'es-ES': "Movedo a la papelera de reciclaje", ru: "Перенесён в мусорный бак", pl: "Przeniesiony do kosza na recykling", 'pt-BR': "Movido para a lixeira" },
  // 已删除
  deleted: { 'zh-CN': '已删除', 'zh-TW': '已刪除', en: 'Deleted', ko: '삭제되었습니다', ja: '削除しました', de: "Gelöscht", fr: "Supprimé", it: "Cancellato", 'es-ES': "Suprimidos", ru: "Удаленный", pl: "Usunięty", 'pt-BR': "Apagado" },
}
