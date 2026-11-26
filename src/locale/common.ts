import type { DomainMessages } from './types'

/**
 * 通用文案：跨页面复用的动作词与状态词。
 *
 * 只放「换了页面也一样叫这个」的词。带上下文的文案（例如「删除 MOD 包」）
 * 属于具体域，不要为了复用把语义磨平。
 */
export const common: DomainMessages = {
  'action.ok': { 'zh-CN': '确定', 'zh-TW': '確定', en: 'OK', ko: '확인', ja: 'OK', de: "OK", fr: "OK", it: "OK", 'es-ES': "Aceptar", ru: "ОК", pl: "OK", 'pt-BR': "OK" },
  'action.cancel': { 'zh-CN': '取消', 'zh-TW': '取消', en: 'Cancel', ko: '취소', ja: 'キャンセル', de: "Abbrechen", fr: "Annuler", it: "Annulla", 'es-ES': "Cancelar", ru: "Отмена", pl: "Anuluj", 'pt-BR': "Cancelar" },
  'action.save': { 'zh-CN': '保存', 'zh-TW': '儲存', en: 'Save', ko: '저장', ja: '保存', de: "Speichern", fr: "Enregistrer", it: "Salva", 'es-ES': "Guardar", ru: "Сохранить", pl: "Zapisz", 'pt-BR': "Salvar" },
  'action.delete': { 'zh-CN': '删除', 'zh-TW': '刪除', en: 'Delete', ko: '삭제', ja: '削除', de: "Löschen", fr: "Supprimer", it: "Elimina", 'es-ES': "Eliminar", ru: "Удалить", pl: "Usuń", 'pt-BR': "Excluir" },
  'action.open': { 'zh-CN': '打开', 'zh-TW': '開啟', en: 'Open', ko: '열기', ja: '開く', de: "Öffnen", fr: "Ouvrir", it: "Apri", 'es-ES': "Abrir", ru: "Открыть", pl: "Otwórz", 'pt-BR': "Abrir" },
  'action.retry': { 'zh-CN': '重试', 'zh-TW': '重試', en: 'Retry', ko: '다시 시도', ja: '再試行', de: "Erneut versuchen", fr: "Réessayer", it: "Riprova", 'es-ES': "Reintentar", ru: "Повторить", pl: "Spróbuj ponownie", 'pt-BR': "Tentar novamente" },
  'action.refresh': { 'zh-CN': '刷新', 'zh-TW': '重新整理', en: 'Refresh', ko: '새로 고침', ja: '更新', de: "Aktualisieren", fr: "Actualiser", it: "Aggiorna", 'es-ES': "Actualizar", ru: "Обновить список", pl: "Odśwież", 'pt-BR': "Atualizar lista" },
  'action.refetch': { 'zh-CN': '重新获取', 'zh-TW': '重新取得', en: 'Refresh', ko: '다시 가져오기', ja: '再取得', de: 'Neu abrufen', fr: 'Actualiser', it: 'Aggiorna', 'es-ES': 'Volver a consultar', ru: 'Обновить', pl: 'Pobierz ponownie', 'pt-BR': 'Buscar novamente' },
  'action.install': { 'zh-CN': '安装', 'zh-TW': '安裝', en: 'Install', ko: '설치', ja: 'インストール', de: "Installieren", fr: "Installer", it: "Installa", 'es-ES': "Instalar", ru: "Установить", pl: "Zainstaluj", 'pt-BR': "Instalar" },
  'action.reinstall': { 'zh-CN': '重装', 'zh-TW': '重新安裝', en: 'Reinstall', ko: '재설치', ja: '再インストール', de: "Neu installieren", fr: "Réinstaller", it: "Reinstalla", 'es-ES': "Reinstalar", ru: "Переустановить", pl: "Zainstaluj ponownie", 'pt-BR': "Reinstalar" },
  'action.update': { 'zh-CN': '更新', 'zh-TW': '更新', en: 'Update', ko: '업데이트', ja: '更新', de: "Aktualisieren", fr: "Mettre à jour", it: "Aggiorna", 'es-ES': "Actualizar", ru: "Обновить", pl: "Zaktualizuj", 'pt-BR': "Atualizar" },
  'action.copy': { 'zh-CN': '复制', 'zh-TW': '複製', en: 'Copy', ko: '복사', ja: 'コピー', de: "Kopieren", fr: "Copier", it: "Copia", 'es-ES': "Copiar", ru: "Копировать", pl: "Kopiuj", 'pt-BR': "Copiar" },
  'action.copied': { 'zh-CN': '已复制', 'zh-TW': '已複製', en: 'Copied', ko: '복사되었습니다', ja: 'コピーしました', de: "Kopiert", fr: "Copié", it: "Copiato", 'es-ES': "Copiado", ru: "Скопировано", pl: "Skopiowano", 'pt-BR': "Copiado" },
  'action.restore': { 'zh-CN': '恢复', 'zh-TW': '還原', en: 'Restore', ko: '복원', ja: '復元', de: "Wiederherstellen", fr: "Restaurer", it: "Ripristina", 'es-ES': "Restaurar", ru: "Восстановить", pl: "Przywróć", 'pt-BR': "Restaurar" },
  'action.done': { 'zh-CN': '完成', 'zh-TW': '完成', en: 'Done', ko: '완료', ja: '完了', de: "Fertig", fr: "Terminé", it: "Fatto", 'es-ES': "Listo", ru: "Готово", pl: "Gotowe", 'pt-BR': "Concluído" },
  'state.enabled': { 'zh-CN': '已启用', 'zh-TW': '已啟用', en: 'Enabled', ko: '활성화됨', ja: '有効', de: "Aktiviert", fr: "Activé", it: "Abilitato", 'es-ES': "Activado", ru: "Включено", pl: "Włączono", 'pt-BR': "Ativado" },
  'state.disabled': { 'zh-CN': '已禁用', 'zh-TW': '已禁用', en: 'Disabled', ko: '비활성화됨', ja: '無効', de: "Deaktiviert", fr: "Désactivé", it: "Disabilitato", 'es-ES': "Desactivado", ru: "Отключено", pl: "Wyłączono", 'pt-BR': "Desativado" },
  'state.installed': { 'zh-CN': '已安装', 'zh-TW': '已安裝', en: 'Installed', ko: '설치됨', ja: 'インストール済み', de: "Installiert", fr: "Installé", it: "Installato", 'es-ES': "Instalado", ru: "Установлено", pl: "Zainstalowano", 'pt-BR': "Instalado" },
  'state.notInstalled': { 'zh-CN': '未安装', 'zh-TW': '未安裝', en: 'Not installed', ko: '설치되지 않음', ja: '未インストール', de: "Nicht installiert", fr: "Non installé", it: "Non installato", 'es-ES': "No instalado", ru: "Не установлено", pl: "Nie zainstalowano", 'pt-BR': "Não instalado" },
  'state.version': { 'zh-CN': '版本', 'zh-TW': '版本', en: 'Version', ko: '버전', ja: 'バージョン', de: "Version", fr: "Version", it: "Versione", 'es-ES': "Versión", ru: "Версия", pl: "Wersja", 'pt-BR': "Versão" },
  'state.versionUnknown': { 'zh-CN': '版本未知', 'zh-TW': '版本未知', en: 'Unknown version', ko: '버전을 알 수 없음', ja: 'バージョン不明', de: "Unbekannte Version", fr: "Version inconnue", it: "Versione sconosciuta", 'es-ES': "Versión desconocida", ru: "Неизвестная версия", pl: "Nieznana wersja", 'pt-BR': "Versão desconhecida" },
  'loading': { 'zh-CN': '加载中…', 'zh-TW': '載入中…', en: 'Loading…', ko: '불러오는 중…', ja: '読み込み中…', de: "Wird geladen…", fr: "Chargement…", it: "Caricamento…", 'es-ES': "Cargando…", ru: "Загрузка…", pl: "Wczytywanie…", 'pt-BR': "Carregando…" },
  other: { 'zh-CN': '其他', 'zh-TW': '其他', en: 'Other', ko: '기타', ja: 'その他', de: "andere", fr: "Autres", it: "Altri", 'es-ES': "Otros", ru: "другой", pl: "Inne", 'pt-BR': "Outros" },
}
