//! Lossless UnityFS compression rewrite; serialized objects and CAB names stay unchanged.
use super::*;

pub(crate) fn to_lz4hc3(
    bytes: &[u8],
    progress: &dyn Fn(usize, usize),
) -> Result<Option<Vec<u8>>, ParseError> {
    let layout = bundle_layout(bytes)?;
    if !layout.blocks.iter().any(|b| b.2 & 63 == 1) {
        return Ok(None);
    }
    let mut header = BundleReader { bytes, pos: 8 };
    let version = header.u32()?;
    let _ = header.cstr()?;
    let _ = header.cstr()?;
    let size_offset = header.pos;
    let _ = header.i64()?;
    let info_size = header.u32()? as usize;
    let info_raw_size = header.u32()? as usize;
    let flags = header.u32()?;
    // Match FastInit's supported independent UnityFS layout; reject unknown flags.
    if version != 6 || flags & !0xff != 0 || flags & 0x40 == 0 {
        return Err(ParseError::Corrupt("unsupported conversion layout".into()));
    }
    if layout.blocks.len() != 1 || layout.blocks[0].2 & !0x7f != 0 {
        return Err(ParseError::Corrupt("unsupported conversion blocks".into()));
    }
    let info_start = if flags & 0x80 != 0 {
        bytes
            .len()
            .checked_sub(info_size)
            .ok_or_else(|| ParseError::Corrupt("info extent".into()))?
    } else {
        header.pos
    };
    let info = decompress(
        &bytes[info_start..info_start + info_size],
        info_raw_size,
        Compression::from_flags(flags)?,
    )?;
    let mut cursor = BundleReader {
        bytes: &info,
        pos: 16,
    };
    let count = cursor.u32()? as usize;
    cursor.take(
        count
            .checked_mul(10)
            .ok_or_else(|| ParseError::Corrupt("block count".into()))?,
    )?;
    let directory = &info[cursor.pos..];
    if layout.directory.iter().any(|(_, offset, size)| {
        offset
            .checked_add(*size)
            .is_none_or(|end| end > layout.total)
    }) {
        return Err(ParseError::Corrupt("directory extent".into()));
    }
    let data_end = layout
        .data_start
        .checked_add(layout.blocks.iter().map(|b| b.1).sum::<usize>());
    let expected_end = if flags & 0x80 != 0 {
        info_start
    } else {
        bytes.len()
    };
    if data_end != Some(expected_end) {
        return Err(ParseError::Corrupt("data extent".into()));
    }
    let mut data = Vec::with_capacity(layout.total);
    let mut offset = layout.data_start;
    for (size, compressed, kind) in &layout.blocks {
        let end = offset
            .checked_add(*compressed)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| ParseError::Corrupt("block range".into()))?;
        let block = decompress(
            &bytes[offset..end],
            *size,
            Compression::from_flags(*kind as u32)?,
        )?;
        if block.len() != *size {
            return Err(ParseError::Corrupt("block size".into()));
        }
        data.extend(block);
        offset = end;
        progress(data.len(), layout.total * 2);
    }
    let mut encoded = Vec::new();
    let mut records = Vec::new();
    let mut done = 0;
    for block in data.chunks(128 * 1024) {
        // Raw LZ4HC level 3 blocks, without a length prefix or frame header:
        // UnityFS already stores the decoded length in its block table.
        let compressed = lz4::block::compress(
            block,
            Some(lz4::block::CompressionMode::HIGHCOMPRESSION(3)),
            false,
        )
        .map_err(|err| ParseError::Corrupt(format!("LZ4HC3 compression: {err}")))?;
        let (payload, kind) = if compressed.len() < block.len() {
            (compressed.as_slice(), 3u16)
        } else {
            (block, 0u16)
        };
        records.extend_from_slice(&(block.len() as u32).to_be_bytes());
        records.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        records.extend_from_slice(&kind.to_be_bytes());
        encoded.extend_from_slice(payload);
        done += block.len();
        progress(layout.total + done, layout.total * 2);
    }
    let mut output_info = info[..16].to_vec();
    output_info.extend_from_slice(&((records.len() / 10) as u32).to_be_bytes());
    output_info.extend(records);
    output_info.extend_from_slice(directory);
    // FastInit writes uncompressed metadata at the front; retain hash and raw directory records.
    let compressed_info = output_info.clone();
    let mut output = bytes[..size_offset].to_vec();
    output.extend_from_slice(&0i64.to_be_bytes());
    output.extend_from_slice(&(compressed_info.len() as u32).to_be_bytes());
    output.extend_from_slice(&(output_info.len() as u32).to_be_bytes());
    // Front metadata is uncompressed, data blocks are independently decompressible.
    output.extend_from_slice(&(flags & 0x40).to_be_bytes());
    output.extend(compressed_info);
    output.extend(encoded);
    let size = output.len() as i64;
    output[size_offset..size_offset + 8].copy_from_slice(&size.to_be_bytes());
    // Compare every internal file, including streams, before permitting replacement.
    let rewritten = bundle_layout(&output)?;
    use sha2::{Digest, Sha256};
    let expected_hash = Sha256::digest(&data);
    let mut verified_hash = Sha256::new();
    let mut verified_length = 0usize;
    let mut position = rewritten.data_start;
    for (plain, packed, kind) in &rewritten.blocks {
        let block = decompress(
            &output[position..position + packed],
            *plain,
            Compression::from_flags(*kind as u32)?,
        )?;
        verified_length += block.len();
        verified_hash.update(&block);
        position += packed;
    }
    if verified_length != data.len()
        || verified_hash.finalize() != expected_hash
        || rewritten.directory != layout.directory
    {
        return Err(ParseError::Corrupt("recompression content mismatch".into()));
    }
    Ok(Some(output))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn fixture(data: &[u8], at_end: bool) -> Vec<u8> {
        use std::io::Write;
        let options = xz2::stream::LzmaOptions::new_preset(1).unwrap();
        let stream = xz2::stream::Stream::new_lzma_encoder(&options).unwrap();
        let mut encoder = xz2::write::XzEncoder::new_stream(Vec::new(), stream);
        encoder.write_all(data).unwrap();
        let encoded = encoder.finish().unwrap();
        let mut packed = encoded[..5].to_vec();
        packed.extend_from_slice(&encoded[13..]);
        let mut info = vec![0; 16];
        info.extend_from_slice(&1u32.to_be_bytes());
        info.extend_from_slice(&(data.len() as u32).to_be_bytes());
        info.extend_from_slice(&(packed.len() as u32).to_be_bytes());
        info.extend_from_slice(&0x41u16.to_be_bytes());
        info.extend_from_slice(&1u32.to_be_bytes());
        info.extend_from_slice(&0i64.to_be_bytes());
        info.extend_from_slice(&(data.len() as i64).to_be_bytes());
        info.extend_from_slice(&4u32.to_be_bytes());
        info.extend_from_slice(b"CAB-test\0");
        let mut bundle = b"UnityFS\0".to_vec();
        bundle.extend_from_slice(&6u32.to_be_bytes());
        bundle.extend_from_slice(b"5.x.x\02018.4.0f1\0");
        let size_offset = bundle.len();
        bundle.extend_from_slice(&0i64.to_be_bytes());
        bundle.extend_from_slice(&(info.len() as u32).to_be_bytes());
        bundle.extend_from_slice(&(info.len() as u32).to_be_bytes());
        bundle.extend_from_slice(&(if at_end { 0xc0u32 } else { 0x40u32 }).to_be_bytes());
        if at_end {
            bundle.extend(packed);
            bundle.extend(info);
        } else {
            bundle.extend(info);
            bundle.extend(packed);
        }
        let size = bundle.len() as i64;
        bundle[size_offset..size_offset + 8].copy_from_slice(&size.to_be_bytes());
        bundle
    }
    #[test]
    fn front_and_end_metadata_preserve_payload_and_skip_second_conversion() {
        let data = (0..400_000).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        for at_end in [false, true] {
            let source = fixture(&data, at_end);
            let output = to_lz4hc3(&source, &|_, _| {}).unwrap().unwrap();
            assert_eq!(
                read_bundle_entries(&source).unwrap(),
                read_bundle_entries(&output).unwrap()
            );
            assert_eq!(bundle_layout(&output).unwrap().blocks.len(), 4);
            assert!(
                bundle_layout(&output)
                    .unwrap()
                    .blocks
                    .iter()
                    .all(|b| b.2 == 3)
            );
            let layout = bundle_layout(&output).unwrap();
            let mut position = layout.data_start;
            for (block, (_, packed, _)) in data.chunks(128 * 1024).zip(&layout.blocks) {
                let expected = lz4::block::compress(
                    block,
                    Some(lz4::block::CompressionMode::HIGHCOMPRESSION(3)),
                    false,
                )
                .unwrap();
                assert_eq!(&output[position..position + packed], expected);
                position += packed;
            }
            assert!(to_lz4hc3(&output, &|_, _| {}).unwrap().is_none());
            // Existing ordinary LZ4 blocks use the same decoder and remain untouched.
            let mut ordinary = output.clone();
            let mut header = BundleReader {
                bytes: &ordinary,
                pos: 8,
            };
            header.u32().unwrap();
            header.cstr().unwrap();
            header.cstr().unwrap();
            header.i64().unwrap();
            header.u32().unwrap();
            header.u32().unwrap();
            header.u32().unwrap();
            let info_start = header.pos;
            for i in 0..layout.blocks.len() {
                let flag = info_start + 20 + i * 10 + 8;
                ordinary[flag..flag + 2].copy_from_slice(&2u16.to_be_bytes());
            }
            assert!(to_lz4hc3(&ordinary, &|_, _| {}).unwrap().is_none());
        }
    }
    #[test]
    fn truncated_bundle_cannot_convert() {
        let mut source = fixture(&vec![7; 1024], false);
        source.pop();
        assert!(to_lz4hc3(&source, &|_, _| {}).is_err());
    }
    #[test]
    fn incompressible_block_stays_raw() {
        let mut state = 0x12345678u32;
        let data = (0..128 * 1024)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect::<Vec<_>>();
        let source = fixture(&data, false);
        let output = to_lz4hc3(&source, &|_, _| {}).unwrap().unwrap();
        assert_eq!(bundle_layout(&output).unwrap().blocks[0].2, 0);
        assert_eq!(
            read_bundle_entries(&source).unwrap(),
            read_bundle_entries(&output).unwrap()
        );
        assert!(to_lz4hc3(&output, &|_, _| {}).unwrap().is_none());
    }
    #[test]
    #[ignore = "requires the installed pear info fixture"]
    fn real_lzma_info_recompresses_without_changing_entries() {
        let path = std::env::var_os("OC2_LZMA_INFO_FIXTURE").map(std::path::PathBuf::from).unwrap_or_else(||
            "E:/SteamLibrary/steamapps/common/Overcooked! 2/BepInEx/plugins/OC2DIYLevel/levels/pear_v11.0(菊花梨）/info_pear".into()
        );
        let bytes = std::fs::read(path).expect("real info fixture");
        let source = bytes.clone();
        let output = to_lz4hc3(&source, &|_, _| {})
            .expect("recompress")
            .expect("LZMA fixture");
        assert_ne!(bytes, output);
        assert_eq!(
            read_bundle_entries(&source).unwrap(),
            read_bundle_entries(&output).unwrap()
        );
        assert_eq!(
            parse_level_set(&source).unwrap(),
            parse_level_set(&output).unwrap()
        );
    }
}
