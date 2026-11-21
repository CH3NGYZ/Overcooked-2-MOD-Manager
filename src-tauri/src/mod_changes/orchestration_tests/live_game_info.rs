//! Small UnityFS v6 fixture with a single typed LevelSetInfo object. Using bytes
//! generated here keeps ZIP parsing in the test and avoids a downloaded map.

pub(super) fn bundle(name: &str, author: &str, version: &str) -> Vec<u8> {
    let data = serialized_file([
        ("levelSetName", name),
        ("author", author),
        ("version", version),
    ]);
    let size = (data.len() as u32).to_be_bytes();
    let mut table = vec![0; 16]; // block information hash
    table.extend(1u32.to_be_bytes()); // block count
    table.extend(size); // raw size
    table.extend(size); // stored size
    table.extend(0u16.to_be_bytes()); // uncompressed
    table.extend(1u32.to_be_bytes()); // file count
    table.extend(0u64.to_be_bytes()); // first file starts at block offset zero
    table.extend((data.len() as u64).to_be_bytes());
    table.extend(0u32.to_be_bytes()); // node flags
    table.extend(b"CAB-fixture\0");

    let mut bundle = b"UnityFS\0\0\0\0\x06".to_vec();
    bundle.extend(b"5.x.x\0");
    bundle.extend(b"2020.3.0f1\0");
    let size_offset = bundle.len();
    bundle.extend([0; 8]); // total bundle size, filled in below
    bundle.extend((table.len() as u32).to_be_bytes()); // stored table size
    bundle.extend((table.len() as u32).to_be_bytes()); // raw table size
    bundle.extend(0x40u32.to_be_bytes()); // inline, uncompressed table
    bundle.extend(table);
    bundle.extend(data);
    let size = (bundle.len() as u64).to_be_bytes();
    bundle[size_offset..size_offset + 8].copy_from_slice(&size);
    bundle
}

fn serialized_file(fields: [(&str, &str); 3]) -> Vec<u8> {
    let mut object = Vec::new();
    for (_, value) in fields {
        object.extend((value.len() as u32).to_le_bytes());
        object.extend(value.as_bytes());
        object.resize(object.len().next_multiple_of(4), 0);
    }

    let mut strings = Vec::new();
    let mut nodes = Vec::new();
    let root = [(0, "MonoBehaviour", "Base")];
    for (index, (depth, kind, name)) in root
        .into_iter()
        .chain(fields.map(|(name, _)| (1, "string", name)))
        .enumerate()
    {
        nodes.extend(1u16.to_le_bytes()); // node version
        nodes.extend([depth, 0]); // depth and array flag
        for text in [kind, name] {
            nodes.extend((strings.len() as u32).to_le_bytes());
            strings.extend(text.as_bytes());
            strings.push(0);
        }
        nodes.extend((-1i32).to_le_bytes()); // variable size
        nodes.extend((index as u32).to_le_bytes());
        nodes.extend(0u32.to_le_bytes()); // node flags
    }

    const HEADER: usize = 20;
    let mut file = vec![0; HEADER]; // little-endian marker and padding are zero
    file.extend(b"2020.3.0f1\0");
    file.extend(19u32.to_le_bytes()); // Windows 64-bit target
    file.push(1); // type trees enabled
    file.extend(1u32.to_le_bytes()); // type count
    file.extend(114u32.to_le_bytes()); // MonoBehaviour
    file.push(0); // stripped flag
    file.extend((-1i16).to_le_bytes()); // script type index
    file.extend([0; 32]); // script ID and type hash
    file.extend(4u32.to_le_bytes()); // type-tree node count (root + 3 fields)
    file.extend((strings.len() as u32).to_le_bytes());
    file.extend(nodes);
    file.extend(strings);
    file.extend(1u32.to_le_bytes()); // object count
    file.resize(file.len().next_multiple_of(4), 0);
    file.extend(1u64.to_le_bytes()); // object path ID
    file.extend(0u32.to_le_bytes()); // offset relative to the data section
    file.extend((object.len() as u32).to_le_bytes());
    file.extend(0u32.to_le_bytes()); // type index
    file.extend([0; 9]); // zero script and external references, empty user info
    let metadata_size = file.len() - HEADER;
    file.resize(file.len().next_multiple_of(16), 0);
    let data_offset = file.len();
    file.extend(object);
    let header = [
        metadata_size as u32,
        file.len() as u32,
        17,
        data_offset as u32,
    ];
    for (field, value) in file[..16].chunks_exact_mut(4).zip(header) {
        field.copy_from_slice(&value.to_be_bytes());
    }
    file
}
