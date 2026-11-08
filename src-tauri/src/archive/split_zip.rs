//! Convert classic multi-disk ZIPs to one ZIP. Offsets in a split ZIP are
//! relative to the disk named in each directory entry, not the concatenation.
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

const INVALID: &str = "download.invalidSplitZip";
const UNSUPPORTED: &str = "download.unsupportedSplitZip";

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// `paths` must already be ordered z01, z02, ..., zip. The caller owns output
/// cleanup on every failure, including cancellation while copying.
pub(super) fn reassemble(paths: &[impl AsRef<Path>], output: &Path) -> Result<(), String> {
    if paths.len() < 2 || paths.len() > 100 {
        return Err(INVALID.into());
    }
    let mut file = File::create(output).map_err(|e| e.to_string())?;
    let mut bases = Vec::with_capacity(paths.len());
    let mut lengths = Vec::with_capacity(paths.len());
    let mut size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    for path in paths {
        bases.push(size);
        let mut source = File::open(path).map_err(|e| e.to_string())?;
        let length = source.metadata().map_err(|e| e.to_string())?.len();
        if length == 0 {
            return Err(INVALID.into());
        }
        lengths.push(length);
        loop {
            if super::is_download_cancelled() {
                return Err("download.cancelled".into());
            }
            let count = source.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            file.write_all(&buffer[..count])
                .map_err(|e| e.to_string())?;
            size = size.checked_add(count as u64).ok_or(UNSUPPORTED)?;
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(output)
        .map_err(|e| e.to_string())?;
    let tail_len = size.min(65557) as usize;
    file.seek(SeekFrom::End(-(tail_len as i64)))
        .map_err(|e| e.to_string())?;
    let mut tail = vec![0; tail_len];
    file.read_exact(&mut tail).map_err(|_| INVALID)?;
    let end = (0..tail_len.saturating_sub(21))
        .rev()
        .find(|&at| {
            tail[at..at + 4] == *b"PK\x05\x06"
                && at + 22 + u16_at(&tail, at + 20) as usize == tail_len
        })
        .ok_or(INVALID)?;
    let mut eocd = tail[end..end + 22].to_vec();
    let last_disk = u16_at(&eocd, 4);
    let cd_disk = u16_at(&eocd, 6);
    let entries = u16_at(&eocd, 10);
    let cd_size = u32_at(&eocd, 12);
    let cd_offset = u32_at(&eocd, 16);
    if last_disk == u16::MAX
        || cd_disk == u16::MAX
        || entries == u16::MAX
        || cd_size == u32::MAX
        || cd_offset == u32::MAX
    {
        return Err(UNSUPPORTED.into());
    }
    let end_offset = size - tail_len as u64 + end as u64;
    if last_disk as usize != paths.len() - 1
        || cd_disk > last_disk
        || cd_offset as u64 >= lengths[cd_disk as usize]
        || end_offset < bases[last_disk as usize]
    {
        return Err(INVALID.into());
    }
    let directory = bases[cd_disk as usize] + cd_offset as u64;
    let directory_end = directory.checked_add(cd_size as u64).ok_or(INVALID)?;
    if directory_end != end_offset || directory > u32::MAX as u64 {
        return Err(UNSUPPORTED.into());
    }
    let mut cursor = directory;
    for _ in 0..entries {
        if super::is_download_cancelled() {
            return Err("download.cancelled".into());
        }
        if cursor + 46 > directory_end {
            return Err(INVALID.into());
        }
        file.seek(SeekFrom::Start(cursor))
            .map_err(|e| e.to_string())?;
        let mut header = [0u8; 46];
        file.read_exact(&mut header).map_err(|_| INVALID)?;
        if header[..4] != *b"PK\x01\x02" {
            return Err(INVALID.into());
        }
        let disk = u16_at(&header, 34);
        let offset = u32_at(&header, 42);
        if disk == u16::MAX
            || offset == u32::MAX
            || u32_at(&header, 20) == u32::MAX
            || u32_at(&header, 24) == u32::MAX
        {
            return Err(UNSUPPORTED.into());
        }
        if disk > last_disk || offset as u64 >= lengths[disk as usize] {
            return Err(INVALID.into());
        }
        let absolute = bases[disk as usize] + offset as u64;
        if absolute >= directory || absolute > u32::MAX as u64 {
            return Err(INVALID.into());
        }
        file.seek(SeekFrom::Start(absolute))
            .map_err(|e| e.to_string())?;
        let mut signature = [0u8; 4];
        file.read_exact(&mut signature).map_err(|_| INVALID)?;
        if signature != *b"PK\x03\x04" {
            return Err(INVALID.into());
        }
        header[34..36].copy_from_slice(&0u16.to_le_bytes());
        header[42..46].copy_from_slice(&(absolute as u32).to_le_bytes());
        file.seek(SeekFrom::Start(cursor))
            .map_err(|e| e.to_string())?;
        file.write_all(&header).map_err(|e| e.to_string())?;
        cursor += 46
            + u16_at(&header, 28) as u64
            + u16_at(&header, 30) as u64
            + u16_at(&header, 32) as u64;
        if cursor > directory_end {
            return Err(INVALID.into());
        }
    }
    if cursor != directory_end {
        return Err(INVALID.into());
    }
    eocd[4..8].fill(0);
    eocd[8..10].copy_from_slice(&entries.to_le_bytes());
    eocd[16..20].copy_from_slice(&(directory as u32).to_le_bytes());
    file.seek(SeekFrom::Start(end_offset))
        .map_err(|e| e.to_string())?;
    file.write_all(&eocd).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    super::validate_zip(output)
}
