import type { DomainMessages } from './types'

/**
 * common 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const uiCommon: DomainMessages = {
  // 保存并验证
  saveAndVerify: { 'zh-CN': '保存并验证', 'zh-TW': '儲存並驗證', en: 'Save and verify', ko: '저장 후 검증', ja: '保存して検証', de: "Speichern und Verifizieren", fr: "Enregistrer et vérifier", it: "Salvare e verificare", 'es-ES': "Guardar y verificar", ru: "Сохранить и проверить", pl: "Zapisz i weryfikuj", 'pt-BR': "Gravar e verificar" },
  // 恢复默认
  reset: { 'zh-CN': '恢复默认', 'zh-TW': '還原預設', en: 'Reset', ko: '기본값 복원', ja: '既定に戻す', de: "Zurücksetzen", fr: "Réinitialiser", it: "Ripristino", 'es-ES': "Restablecer", ru: "Сброс", pl: "Przywróć", 'pt-BR': "Reiniciar" },
  // 按此状态筛选列表，再点一次取消
  filterByStatus: { 'zh-CN': '按此状态筛选列表，再点一次取消', 'zh-TW': '依此狀態篩選清單，再點一次取消', en: 'Filter the list by this status; click again to clear', ko: '이 상태로 목록을 필터링합니다. 다시 클릭하면 해제됩니다', ja: 'この状態で一覧を絞り込みます。もう一度クリックで解除', de: "Filtern Sie die Liste nach diesem Status; klicken Sie erneut, um zu löschen", fr: "Filtrer la liste par cet état ; cliquez à nouveau pour effacer", it: "Filtra l'elenco per questo stato; fare clic di nuovo per cancellare", 'es-ES': "Filtrar la lista por este estado; haga clic de nuevo para despejar", ru: "Отфильтровать список по этому статусу; нажмите еще раз, чтобы очистить", pl: "Filtruj listę przez ten status; kliknij ponownie, aby wyczyścić", 'pt-BR': "Filtrar a lista por este estado; clique novamente para limpar" },
  // 当前状态下没有可显示的项
  noStatusMatch: { 'zh-CN': '当前状态下没有可显示的项，再点一次标签取消筛选。', 'zh-TW': '目前狀態下沒有可顯示的項目，再點一次標籤取消篩選。', en: 'Nothing to show for this status. Click the tag again to clear the filter.', ko: '이 상태로 표시할 항목이 없습니다. 태그를 다시 클릭하면 필터가 해제됩니다.', ja: 'この状態では表示する項目がありません。タグをもう一度クリックすると解除されます。', de: "Nichts für diesen Status anzeigen. Klicken Sie erneut auf das Tag, um den Filter zu löschen.", fr: "Rien à afficher pour cet état. Cliquez à nouveau sur la balise pour effacer le filtre.", it: "Non c'è niente da mostrare per questo stato. Fare clic sul tag di nuovo per cancellare il filtro.", 'es-ES': "Nada que mostrar para este estado. Haga clic en la etiqueta de nuevo para limpiar el filtro.", ru: "Нечего показывать для этого статуса. Нажмите тег снова, чтобы очистить фильтр.", pl: "Nic do pokazania dla tego stanu. Kliknij ponownie znacznik, aby wyczyścić filtr.", 'pt-BR': "Nada a mostrar para este estado. Carregue novamente na etiqueta para limpar o filtro." },
  // 已保存
  saved: { 'zh-CN': '已保存', 'zh-TW': '已儲存', en: 'Saved', ko: '저장됨', ja: '保存しました', de: 'Gespeichert', fr: 'Enregistré', it: 'Salvato', 'es-ES': 'Guardado', ru: 'Сохранено', pl: 'Zapisano', 'pt-BR': 'Salvo' },
  // 游戏运行中
  gameRunning: { 'zh-CN': '游戏运行中', 'zh-TW': '遊戲執行中', en: 'Game running', ko: '게임 실행 중', ja: 'ゲーム実行中', de: "Spiellauf", fr: "Jeu lancé", it: "Gioco in esecuzione", 'es-ES': "Juego de correr", ru: "Игра работает", pl: "Uruchomienie gry", 'pt-BR': "Jogo em execução" },
  // 取消下载
  cancelDownload: { 'zh-CN': '取消下载', 'zh-TW': '取消下載', en: 'Cancel', ko: '다운로드 취소', ja: 'ダウンロード中止', de: "Abbrechen", fr: "Annuler", it: "Annulla", 'es-ES': "Cancelar", ru: "Отмена", pl: "Anuluj", 'pt-BR': "Cancelar" },
  // 等待大小
  waitingForSize: { 'zh-CN': '等待大小', 'zh-TW': '等待大小', en: 'Waiting for size', ko: '크기 대기', ja: 'サイズ待ち', de: "Warten auf Größe", fr: "Attendre la taille", it: "In attesa di dimensioni", 'es-ES': "Esperando el tamaño", ru: "В ожидании размера", pl: "Oczekiwanie na rozmiar", 'pt-BR': "À espera do tamanho" },
  // 代理
  proxy: { 'zh-CN': '代理', 'zh-TW': '代理', en: 'Proxy', ko: '프록시', ja: 'プロキシ', de: "Proxy oxy", fr: "Voie orale", it: "Destinazione", 'es-ES': "Proxi", ru: "Прокси", pl: "Proxy (Proxy)", 'pt-BR': "ProxyName" },
  // 直连
  direct: { 'zh-CN': '直连', 'zh-TW': '直連', en: 'Direct', ko: '직접 연결', ja: '直接', de: "Direkt", fr: "Directe", it: "Regia", 'es-ES': "Directo", ru: "Прямой эфир", pl: "Bezpośrednie", 'pt-BR': "Directo" },
  // 这个目录不存在或缺少游戏文件
  theFolderIsMissingOrHasNoGameFiles: { 'zh-CN': '这个目录不存在或缺少游戏文件', 'zh-TW': '這個目錄不存在或缺少遊戲檔案', en: 'The folder is missing or has no game files', ko: '폴더가 없거나 게임 파일이 없습니다', ja: 'フォルダーがないか、ゲームファイルがありません', de: "Der Ordner fehlt oder hat keine Spieldateien", fr: "Le dossier est manquant ou n'a pas de fichiers de jeu", it: "La cartella manca o non ha file di gioco", 'es-ES': "La carpeta está desaparecida o no tiene archivos de juego", ru: "Отсутствует папка или нет игровых файлов", pl: "Folder jest brakujący lub nie ma plików gier", 'pt-BR': "A pasta está faltando ou não tem arquivos de jogo" },
  // 保存后自动验证。
  verifiedAutomaticallyAfterSaving: { 'zh-CN': '保存后自动验证。', 'zh-TW': '儲存後自動驗證。', en: 'Verified automatically after saving.', ko: '저장 후 자동으로 검증합니다.', ja: '保存後に自動検証します。', de: "Automatisch nach dem Speichern verifiziert.", fr: "Vérifié automatiquement après enregistrement.", it: "Verificato automaticamente dopo il salvataggio.", 'es-ES': "Verificado automáticamente después de guardar.", ru: "Проверяется автоматически после сохранения.", pl: "Zweryfikowane automatycznie po zapisaniu.", 'pt-BR': "Verificado automaticamente após a gravação." },
  // 文件不存在
  fileNotFound: { 'zh-CN': '文件不存在', 'zh-TW': '檔案不存在', en: 'File not found', ko: '파일 없음', ja: 'ファイルがありません', de: "Datei nicht gefunden", fr: "Fichier non trouvé", it: "File non trovato", 'es-ES': "Archivo no encontrado", ru: "Файл не найден", pl: "Nie znaleziono pliku", 'pt-BR': "Arquivo não encontrado" },
  // 游戏目录已变化，请重新添加模组
  gameDirectoryChangedReAddTheMods: { 'zh-CN': '游戏目录已变化，请重新添加模组', 'zh-TW': '遊戲目錄已變更，請重新新增模組', en: 'Game directory changed; re-add the mods', ko: '게임 폴더가 변경되었습니다. 모드를 다시 추가하세요', ja: 'ゲームフォルダーが変更されました。MODを再追加してください', de: "Spielverzeichnis geändert; Neu-Hinzufügen der Mods", fr: "Le répertoire du jeu a changé; ré-ajouter les mods", it: "directory di gioco cambiata; ri-add le mods", 'es-ES': "El directorio del juego cambió; re-add los mods", ru: "Изменился каталог игр, были добавлены новые моды", pl: "Zmieniony katalog gier; ponownie- dodaj mody", 'pt-BR': "A pasta do jogo foi alterada; re- adicionar os mods" },
}
