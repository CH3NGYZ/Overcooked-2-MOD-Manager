import type { DomainMessages } from './types'

/**
 * 元数据批读中间层（`metadata_read.rs`）的错误。
 *
 * 只有一条：某项读取任务在边界处被吞掉的 panic 兜底。正常失败（文件读不出、
 * 解析失败）由各自的域（`assetBundle` / `levelSet` / `mods`）负责，这里只管
 * 「任务没跑完」这种异常终止。
 */
export const metadataRead: DomainMessages = {
  taskFailed: { 'zh-CN': '读取元数据任务异常终止', 'zh-TW': '讀取中繼資料任務異常終止', en: 'A metadata read task terminated unexpectedly.', ko: '메타데이터 읽기 작업이 비정상 종료되었습니다.', ja: 'メタデータ読み取りタスクが異常終了しました。', de: "Eine Metadaten-Leseaufgabe wurde unerwartet beendet.", fr: "Une tâche de lecture des métadonnées s'est terminée de façon inattendue.", it: "Un'attività di lettura dei metadati è terminata inaspettatamente.", 'es-ES': "Una tarea de lectura de metadatos terminó inesperadamente.", ru: "Задача чтения метаданных неожиданно завершилась.", pl: "Metadane odczytywane zadania zakończyły się niespodziewanie.", 'pt-BR': "Uma tarefa de leitura de metadados terminou inesperadamente." },
}
