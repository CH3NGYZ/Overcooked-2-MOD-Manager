//! ZIP info 候选并行读取与条目分类。

use crate::asset_bundle;
use crate::asset_bundle::LevelSetMetadata;
use std::fs;
use std::io::Read;
use std::io::Seek;
use std::path::Path;

/// 一个待解析的 `info` 目录：ZIP 内目录名 + 该目录下全部候选条目。
pub(super) struct InfoDirectory {
    pub(super) directory: String,
    /// 候选条目：ZIP 条目下标 + 条目名（同一目录可能有多个 `info*`）。
    pub(super) candidates: Vec<(usize, String)>,
}

/// 一个目录的解析结果：成功时带元数据，失败时带最后一次失败原因。
struct InfoOutcome {
    metadata: Option<LevelSetMetadata>,
    last_error: String,
}

/// 并行解析各目录的 `info`，返回按目录名排序的 `(目录名, 元数据)`。
///
/// 目录内仍按候选顺序逐个试解析，取第一个能读出关卡集的 —— 实测「04_前置
/// 地图合集」的 `宜/` 里 `info_yi` 是真关卡集、`info_yil` 只是随包资源，
/// 这个语义不能因为并行而改变。任一目录的全部候选都失败时整体报错（带最后
/// 一次原因），与串行版一致。
pub(super) fn parse_zip_info_directories(
    zip_path: &Path,
    directories: &[InfoDirectory],
    workers: usize,
    strict: bool,
    progress: &(dyn Fn(usize, usize, &str) + Sync),
) -> Result<Vec<(String, LevelSetMetadata)>, String> {
    let total = directories.len();
    if total == 0 {
        return Ok(Vec::new());
    }
    let workers = workers.clamp(1, total);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let finished = std::sync::atomic::AtomicUsize::new(0);
    let outcomes: Vec<std::sync::Mutex<Option<InfoOutcome>>> =
        (0..total).map(|_| std::sync::Mutex::new(None)).collect();
    crate::concurrency::run_queue(
        workers,
        || {
            /*
             * 每个线程各自打开一份 ZIP：`ZipArchive` 的读句柄不是 `Sync`，
             * 共用一份就得加锁，等于把并行又串回去。中央目录只读一遍，
             * 代价远小于解析一个 `info`。
             */
            let archive = match fs::File::open(zip_path)
                .map_err(|error| error.to_string())
                .and_then(|file| {
                    zip::ZipArchive::new(file).map_err(|error| {
                        crate::locale_tag::with_params(
                            "archive.zipReadFailed",
                            &[("reason", &error.to_string())],
                        )
                    })
                }) {
                Ok(archive) => Some(archive),
                Err(reason) => {
                    log::warn!(
                        "ZIP 读取句柄打开失败，识别将整体失败：文件={}，原因={}",
                        zip_path.display(),
                        reason
                    );
                    None
                }
            };
            archive
        },
        || {
            let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            (index < total).then_some(index)
        },
        |archive, &index| {
            let directory = &directories[index];
            let mut outcome = InfoOutcome {
                metadata: None,
                last_error: String::new(),
            };
            for (entry_index, entry_name) in &directory.candidates {
                /*
                 * 进度在**开始读这一份**时上报：同一目录有多个候选时
                 * `done` 不变、`current` 换成下一个 `info` 文件名，
                 * 用户能看出在重试而不是卡住。
                 */
                progress(
                    finished.load(std::sync::atomic::Ordering::Relaxed),
                    total,
                    entry_name,
                );
                let Some(archive) = archive.as_mut() else {
                    outcome.last_error = "archive.zipReadFailed".to_string();
                    break;
                };
                match read_zip_entry(archive, *entry_index) {
                    Ok(bytes) => match asset_bundle::parse_level_set(&bytes) {
                        Ok(parsed) => {
                            outcome.metadata = Some(parsed);
                            break;
                        }
                        Err(error) => {
                            log::debug!(
                                "候选 info 不是关卡集，继续试下一个：目录={}，条目={}，原因={}",
                                directory.directory,
                                entry_name,
                                error
                            );
                            outcome.last_error = error.to_string();
                        }
                    },
                    Err(reason) => {
                        log::warn!(
                            "候选 info 读取失败，继续试下一个：目录={}，条目={}，原因={}",
                            directory.directory,
                            entry_name,
                            reason
                        );
                        outcome.last_error = reason;
                    }
                }
            }
            outcome
        },
        |index, outcome| {
            finished.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            *outcomes[index].lock().unwrap_or_else(|e| e.into_inner()) =
                Some(outcome.unwrap_or_else(|| InfoOutcome {
                    metadata: None,
                    last_error: "metadataRead.taskFailed".into(),
                }));
        },
    );
    let mut parsed = Vec::with_capacity(total);
    for (index, directory) in directories.iter().enumerate() {
        let outcome = outcomes[index]
            .lock()
            .ok()
            .and_then(|mut slot| slot.take())
            .unwrap_or_else(|| InfoOutcome {
                metadata: None,
                last_error: String::new(),
            });
        match outcome.metadata {
            Some(metadata) => parsed.push((directory.directory.clone(), metadata)),
            /*
             * `strict` 是历史形态的语义：ZIP 里任何一张卡读不出来都整体报错，
             * 用户看到的是「这个包有问题」而不是「悄悄少了几张」。
             *
             * BepInEx 表层包相反：`levels/` 下可能有作者留下的备份目录、说明
             * 用的 `info_*`，它们本来就不是关卡集，不能因此让整包进不来。
             */
            None if !strict => {
                log::warn!(
                    "ZIP 内目录没有可识别的 info，跳过：目录={}，原因={}",
                    directory.directory,
                    outcome.last_error
                );
            }
            None => {
                return Err(crate::locale_tag::with_params(
                    "levelSet.zipInfoInvalid",
                    &[("reason", &outcome.last_error)],
                ));
            }
        }
    }
    Ok(parsed)
}

/// 读出一个 ZIP 条目的全部字节（`info` 是自包含容器，识别不需要落盘）。
fn read_zip_entry(
    archive: &mut zip::ZipArchive<fs::File>,
    index: usize,
) -> Result<Vec<u8>, String> {
    let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

/// 是否是 macOS 打包产生的垃圾条目。
pub(super) fn is_junk_entry(name: &str) -> bool {
    name.starts_with("__MACOSX/")
        || name.rsplit('/').next().is_some_and(|file| {
            file == ".DS_Store" || file.starts_with("._") || file == "Thumbs.db"
        })
}

/// 是否是关卡集的 `info` 文件（`info` 或 `info_<集名>`）。
pub(super) fn is_info_file(name: &str) -> bool {
    name == "info" || name.starts_with("info_")
}

/// 把 ZIP 条目名拆成 `(目录, 文件名)`。根目录返回空字符串。
pub(super) fn split_entry(name: &str) -> (String, String) {
    match name.rsplit_once('/') {
        Some((directory, file)) => (directory.to_string(), file.to_string()),
        None => (String::new(), name.to_string()),
    }
}

/// 供 ZIP 读取测试用：把内存字节包成可 seek 的读取器。
#[allow(dead_code)]
trait ReadSeek: Read + Seek {}

impl<T: Read + Seek> ReadSeek for T {}
