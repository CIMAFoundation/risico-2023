//! Byte encoders for the HDF5 structures a NetCDF-4 file is made of.
//!
//! Only what NetCDF-4 output needs is here: a version 2 superblock, version 2
//! object headers, the dataspace / datatype / fill value / filter pipeline /
//! layout / attribute / link messages, one global heap collection (for
//! `DIMENSION_LIST`), and the version 1 B-tree that indexes chunks. The
//! choices mirror what netcdf-c itself writes, so every reader of today's
//! files reads these too.

use crate::checksum::jenkins_lookup3;

pub(crate) const UNDEFINED_ADDRESS: u64 = u64::MAX;
pub(crate) const SUPERBLOCK_SIZE: u64 = 48;
const OFFSET_SIZE: u8 = 8;
const LENGTH_SIZE: u8 = 8;
const HDF5_MAGIC: [u8; 8] = [0x89, b'H', b'D', b'F', 0x0d, 0x0a, 0x1a, 0x0a];

pub(crate) const MSG_DATASPACE: u8 = 0x01;
pub(crate) const MSG_LINK_INFO: u8 = 0x02;
pub(crate) const MSG_DATATYPE: u8 = 0x03;
pub(crate) const MSG_FILL_VALUE: u8 = 0x05;
pub(crate) const MSG_LINK: u8 = 0x06;
pub(crate) const MSG_DATA_LAYOUT: u8 = 0x08;
pub(crate) const MSG_GROUP_INFO: u8 = 0x0a;
pub(crate) const MSG_FILTER_PIPELINE: u8 = 0x0b;
pub(crate) const MSG_ATTRIBUTE: u8 = 0x0c;

/// Space allocation time stored in the fill value message.
pub(crate) const ALLOC_TIME_LATE: u8 = 2;
pub(crate) const ALLOC_TIME_INCREMENTAL: u8 = 3;
const FILL_WRITE_TIME_IFSET: u8 = 2 << 2;
const FILL_VALUE_DEFINED: u8 = 0x20;

/// One header message: its type and payload.
pub(crate) struct Message {
    pub type_id: u8,
    pub payload: Vec<u8>,
}

pub(crate) fn align8(value: u64) -> u64 {
    (value + 7) & !7
}

fn size_width_for(value: u64) -> (u8, usize) {
    if value <= u8::MAX as u64 {
        (0, 1)
    } else if value <= u16::MAX as u64 {
        (1, 2)
    } else if value <= u32::MAX as u64 {
        (2, 4)
    } else {
        (3, 8)
    }
}

fn push_uvar(out: &mut Vec<u8>, value: u64, width: usize) {
    out.extend_from_slice(&value.to_le_bytes()[..width]);
}

/// Version 2 superblock: no user block, 8-byte offsets and lengths.
pub(crate) fn superblock_v2(root_address: u64, eof_address: u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(SUPERBLOCK_SIZE as usize);
    bytes.extend_from_slice(&HDF5_MAGIC);
    bytes.push(2);
    bytes.push(OFFSET_SIZE);
    bytes.push(LENGTH_SIZE);
    bytes.push(0);
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.extend_from_slice(&UNDEFINED_ADDRESS.to_le_bytes());
    bytes.extend_from_slice(&eof_address.to_le_bytes());
    bytes.extend_from_slice(&root_address.to_le_bytes());
    let checksum = jenkins_lookup3(&bytes);
    bytes.extend_from_slice(&checksum.to_le_bytes());
    bytes
}

/// Version 2 object header holding `messages` in one chunk, no timestamps.
pub(crate) fn object_header_v2(messages: &[Message]) -> Vec<u8> {
    let mut body = Vec::new();
    for message in messages {
        assert!(
            message.payload.len() <= u16::MAX as usize,
            "header message exceeds the 64 KiB a single message can hold"
        );
        body.push(message.type_id);
        body.extend_from_slice(&(message.payload.len() as u16).to_le_bytes());
        body.push(0);
        body.extend_from_slice(&message.payload);
    }
    let (size_flag, size_width) = size_width_for(body.len() as u64);
    let mut bytes = Vec::with_capacity(body.len() + 16);
    bytes.extend_from_slice(b"OHDR");
    bytes.push(2);
    bytes.push(size_flag);
    push_uvar(&mut bytes, body.len() as u64, size_width);
    bytes.extend_from_slice(&body);
    let checksum = jenkins_lookup3(&bytes);
    bytes.extend_from_slice(&checksum.to_le_bytes());
    bytes
}

/// Version 2 dataspace: scalar when `shape` is empty, simple otherwise.
pub(crate) fn dataspace(shape: &[u64], max_shape: Option<&[u64]>) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(4 + shape.len() * 16);
    bytes.push(2);
    bytes.push(shape.len() as u8);
    bytes.push(if max_shape.is_some() { 0x01 } else { 0x00 });
    bytes.push(if shape.is_empty() { 0 } else { 1 });
    for &dim in shape {
        bytes.extend_from_slice(&dim.to_le_bytes());
    }
    if let Some(max_shape) = max_shape {
        for &dim in max_shape {
            bytes.extend_from_slice(&dim.to_le_bytes());
        }
    }
    bytes
}

fn class_word(class: u8, version: u8, flags: u32) -> u32 {
    u32::from(class) | (u32::from(version) << 4) | (flags << 8)
}

/// Little-endian two's complement integer of `size` bytes.
pub(crate) fn datatype_fixed_point(size: u8, signed: bool) -> Vec<u8> {
    let flags = if signed { 0x08 } else { 0 };
    let mut bytes = Vec::with_capacity(12);
    bytes.extend_from_slice(&class_word(0, 1, flags).to_le_bytes());
    bytes.extend_from_slice(&u32::from(size).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&(u16::from(size) * 8).to_le_bytes());
    bytes
}

/// Little-endian IEEE float of 4 or 8 bytes.
pub(crate) fn datatype_float(size: u8) -> Vec<u8> {
    let (exp_location, exp_size, mantissa_size, exp_bias, sign_location) = match size {
        4 => (23u8, 8u8, 23u8, 127u32, 31u32),
        8 => (52u8, 11u8, 52u8, 1023u32, 63u32),
        other => panic!("unsupported float width {other}"),
    };
    // Bit 5: implied mantissa normalisation (IEEE); bits 8-15: sign bit position.
    let flags = 0x20 | (sign_location << 8);
    let mut bytes = Vec::with_capacity(20);
    bytes.extend_from_slice(&class_word(1, 1, flags).to_le_bytes());
    bytes.extend_from_slice(&u32::from(size).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&(u16::from(size) * 8).to_le_bytes());
    bytes.push(exp_location);
    bytes.push(exp_size);
    bytes.push(0);
    bytes.push(mantissa_size);
    bytes.extend_from_slice(&exp_bias.to_le_bytes());
    bytes
}

/// Fixed-length ASCII string, null terminated, of `size` bytes.
pub(crate) fn datatype_fixed_string(size: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8);
    bytes.extend_from_slice(&class_word(3, 1, 0).to_le_bytes());
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes
}

/// 8-byte object reference.
pub(crate) fn datatype_object_reference() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8);
    bytes.extend_from_slice(&class_word(7, 1, 0).to_le_bytes());
    bytes.extend_from_slice(&u32::from(OFFSET_SIZE).to_le_bytes());
    bytes
}

/// Variable-length sequence of `base` elements, stored in the global heap.
pub(crate) fn datatype_vlen_sequence(base: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8 + base.len());
    bytes.extend_from_slice(&class_word(9, 1, 0).to_le_bytes());
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(base);
    bytes
}

pub(crate) const REFERENCE_LIST_ENTRY_SIZE: usize = 16;

/// The compound `{dataset: object reference, dimension: u32}` that the HDF5
/// dimension scale API stores in `REFERENCE_LIST`.
pub(crate) fn datatype_reference_list() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&class_word(6, 3, 2).to_le_bytes());
    bytes.extend_from_slice(&(REFERENCE_LIST_ENTRY_SIZE as u32).to_le_bytes());
    // Version 3 members: null-terminated name, offset in as few bytes as the
    // compound size needs (one here), then the member datatype.
    bytes.extend_from_slice(b"dataset\0");
    bytes.push(0);
    bytes.extend_from_slice(&datatype_object_reference());
    bytes.extend_from_slice(b"dimension\0");
    bytes.push(8);
    bytes.extend_from_slice(&datatype_fixed_point(4, true));
    bytes
}

/// One `REFERENCE_LIST` entry.
pub(crate) fn reference_list_entry(dataset_address: u64, dimension: u32) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&dataset_address.to_le_bytes());
    bytes[8..12].copy_from_slice(&dimension.to_le_bytes());
    bytes
}

/// Version 3 fill value message with a defined value.
pub(crate) fn fill_value(value: &[u8], alloc_time: u8) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(6 + value.len());
    bytes.push(3);
    bytes.push(alloc_time | FILL_WRITE_TIME_IFSET | FILL_VALUE_DEFINED);
    bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
    bytes.extend_from_slice(value);
    bytes
}

/// A filter in the pipeline, with its client data.
#[derive(Clone, Debug)]
pub(crate) struct Filter {
    id: u16,
    client_data: Vec<u32>,
}

impl Filter {
    pub fn shuffle(element_size: u32) -> Self {
        Self {
            id: 2,
            client_data: vec![element_size],
        }
    }

    pub fn deflate(level: u32) -> Self {
        Self {
            id: 1,
            client_data: vec![level],
        }
    }
}

/// Version 2 filter pipeline message; filters apply in the given order.
pub(crate) fn filter_pipeline(filters: &[Filter]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.push(2);
    bytes.push(filters.len() as u8);
    for filter in filters {
        bytes.extend_from_slice(&filter.id.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&(filter.client_data.len() as u16).to_le_bytes());
        for value in &filter.client_data {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}

/// Version 3 contiguous layout.
pub(crate) fn layout_contiguous(address: u64, size: u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(18);
    bytes.push(3);
    bytes.push(1);
    bytes.extend_from_slice(&address.to_le_bytes());
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes
}

/// Version 3 chunked layout indexed by the v1 B-tree at `btree_address`.
pub(crate) fn layout_chunked(btree_address: u64, chunk_shape: &[u64], element_size: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(11 + chunk_shape.len() * 4 + 4);
    bytes.push(3);
    bytes.push(2);
    bytes.push((chunk_shape.len() + 1) as u8);
    bytes.extend_from_slice(&btree_address.to_le_bytes());
    for &dim in chunk_shape {
        bytes.extend_from_slice(&(dim as u32).to_le_bytes());
    }
    bytes.extend_from_slice(&element_size.to_le_bytes());
    bytes
}

/// Version 3 attribute message.
pub(crate) fn attribute(name: &str, datatype: &[u8], dataspace: &[u8], raw: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(9 + name.len() + datatype.len() + dataspace.len() + raw.len());
    bytes.push(3);
    bytes.push(0);
    bytes.extend_from_slice(&((name.len() + 1) as u16).to_le_bytes());
    bytes.extend_from_slice(&(datatype.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&(dataspace.len() as u16).to_le_bytes());
    bytes.push(if name.is_ascii() { 0 } else { 1 });
    bytes.extend_from_slice(name.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(datatype);
    bytes.extend_from_slice(dataspace);
    bytes.extend_from_slice(raw);
    bytes
}

/// Link info message for a compact (new-style) group.
pub(crate) fn link_info() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(18);
    bytes.push(0);
    bytes.push(0);
    bytes.extend_from_slice(&UNDEFINED_ADDRESS.to_le_bytes());
    bytes.extend_from_slice(&UNDEFINED_ADDRESS.to_le_bytes());
    bytes
}

pub(crate) fn group_info() -> Vec<u8> {
    vec![0, 0]
}

/// Hard link `name` to the object header at `address`.
pub(crate) fn hard_link(name: &str, address: u64) -> Vec<u8> {
    let (width_flag, width) = size_width_for(name.len() as u64);
    let utf8 = if name.is_ascii() { 0 } else { 0x10 };
    let mut bytes = Vec::with_capacity(12 + name.len());
    bytes.push(1);
    bytes.push(width_flag | utf8);
    if utf8 != 0 {
        bytes.push(1);
    }
    push_uvar(&mut bytes, name.len() as u64, width);
    bytes.extend_from_slice(name.as_bytes());
    bytes.extend_from_slice(&address.to_le_bytes());
    bytes
}

const GLOBAL_HEAP_MIN_COLLECTION_SIZE: u64 = 4096;

/// One global heap collection holding `objects` (1-based indices in order),
/// each referenced once, closed by the free-space object.
pub(crate) fn global_heap_collection(objects: &[Vec<u8>]) -> Vec<u8> {
    assert!(objects.len() < u16::MAX as usize, "too many global heap objects");
    let mut body = Vec::new();
    for (position, object) in objects.iter().enumerate() {
        body.extend_from_slice(&((position + 1) as u16).to_le_bytes());
        body.extend_from_slice(&1u16.to_le_bytes());
        body.extend_from_slice(&[0; 4]);
        body.extend_from_slice(&(object.len() as u64).to_le_bytes());
        body.extend_from_slice(object);
        let padded = align8(object.len() as u64) as usize;
        body.resize(body.len() + padded - object.len(), 0);
    }
    let used = 16 + body.len() as u64;
    let collection_size = align8(used + 16).max(GLOBAL_HEAP_MIN_COLLECTION_SIZE);
    let free_space = collection_size - used;
    body.extend_from_slice(&0u16.to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes());
    body.extend_from_slice(&[0; 4]);
    body.extend_from_slice(&free_space.to_le_bytes());

    let mut bytes = Vec::with_capacity(collection_size as usize);
    bytes.extend_from_slice(b"GCOL");
    bytes.push(1);
    bytes.extend_from_slice(&[0, 0, 0]);
    bytes.extend_from_slice(&collection_size.to_le_bytes());
    bytes.extend_from_slice(&body);
    bytes.resize(collection_size as usize, 0);
    bytes
}

/// A variable-length element pointing at a global heap object.
pub(crate) fn vlen_reference(length: u32, collection_address: u64, index: u32) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[..4].copy_from_slice(&length.to_le_bytes());
    bytes[4..12].copy_from_slice(&collection_address.to_le_bytes());
    bytes[12..].copy_from_slice(&index.to_le_bytes());
    bytes
}

/// One stored chunk, as the B-tree indexes it.
#[derive(Clone, Debug)]
pub(crate) struct ChunkEntry {
    /// Element offset of the chunk's first element in every dimension.
    pub offsets: Vec<u64>,
    /// Where the (filtered) chunk bytes start.
    pub address: u64,
    /// Length of the stored chunk bytes.
    pub size: u32,
}

/// Entries per B-tree node: 2K with the default K of 32 for chunk trees.
const BTREE_NODE_CAPACITY: usize = 64;

fn btree_key_size(rank: usize) -> usize {
    4 + 4 + (rank + 1) * 8
}

pub(crate) fn btree_node_size(rank: usize) -> u64 {
    let header = 4 + 1 + 1 + 2 + 2 * OFFSET_SIZE as usize;
    (header + BTREE_NODE_CAPACITY * OFFSET_SIZE as usize + (BTREE_NODE_CAPACITY + 1) * btree_key_size(rank))
        as u64
}

/// Node count per level, root first, for `chunks` leaves entries.
fn btree_level_widths(chunks: usize) -> Vec<usize> {
    let mut widths = vec![chunks.div_ceil(BTREE_NODE_CAPACITY).max(1)];
    while widths[0] > 1 {
        let parent = widths[0].div_ceil(BTREE_NODE_CAPACITY);
        widths.insert(0, parent);
    }
    widths
}

/// Bytes the whole index of `chunks` chunks occupies.
pub(crate) fn btree_size(chunks: usize, rank: usize) -> u64 {
    let nodes: usize = btree_level_widths(chunks).iter().sum();
    nodes as u64 * btree_node_size(rank)
}

fn btree_key(size: u32, offsets: &[u64]) -> Vec<u8> {
    let mut key = Vec::with_capacity(btree_key_size(offsets.len()));
    key.extend_from_slice(&size.to_le_bytes());
    key.extend_from_slice(&0u32.to_le_bytes());
    for &offset in offsets {
        key.extend_from_slice(&offset.to_le_bytes());
    }
    key.extend_from_slice(&0u64.to_le_bytes());
    key
}

/// The complete version 1 B-tree for `chunks`, written contiguously from
/// `index_address` with the root node first. The chunks must be in
/// ascending row-major offset order, which is how they are appended.
pub(crate) fn btree_v1(chunks: &[ChunkEntry], chunk_shape: &[u64], index_address: u64) -> Vec<u8> {
    let rank = chunk_shape.len();
    let last = chunks.last().expect("a B-tree needs at least one chunk");
    // The closing key of the whole tree sits one chunk past the last one in
    // every dimension, as libhdf5 writes it.
    let closing_offsets: Vec<u64> = last
        .offsets
        .iter()
        .zip(chunk_shape)
        .map(|(offset, extent)| offset + extent)
        .collect();
    let closing_key = btree_key(0, &closing_offsets);

    let mut children: Vec<(Vec<u8>, u64)> = chunks
        .iter()
        .map(|chunk| (btree_key(chunk.size, &chunk.offsets), chunk.address))
        .collect();

    let node_size = btree_node_size(rank) as usize;
    let widths = btree_level_widths(chunks.len());
    let total_nodes: usize = widths.iter().sum();
    let mut nodes = vec![0u8; total_nodes * node_size];
    let node_address = |index: usize| index_address + (index * node_size) as u64;

    let mut level_start = total_nodes;
    for (level, &width) in widths.iter().rev().enumerate() {
        level_start -= width;
        let mut parents = Vec::with_capacity(width);
        for (position, group) in children.chunks(BTREE_NODE_CAPACITY).enumerate() {
            let index = level_start + position;
            let left = if position == 0 { UNDEFINED_ADDRESS } else { node_address(index - 1) };
            let right = if position + 1 == width { UNDEFINED_ADDRESS } else { node_address(index + 1) };
            let upper_key = children
                .get((position + 1) * BTREE_NODE_CAPACITY)
                .map_or(&closing_key, |(key, _)| key);

            let node = &mut nodes[index * node_size..(index + 1) * node_size];
            let mut cursor = 0;
            let mut put = |bytes: &[u8]| {
                node[cursor..cursor + bytes.len()].copy_from_slice(bytes);
                cursor += bytes.len();
            };
            put(b"TREE");
            put(&[1, level as u8]);
            put(&(group.len() as u16).to_le_bytes());
            put(&left.to_le_bytes());
            put(&right.to_le_bytes());
            for (key, address) in group {
                put(key);
                put(&address.to_le_bytes());
            }
            put(upper_key);

            parents.push((group[0].0.clone(), node_address(index)));
        }
        children = parents;
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn superblock_matches_the_reference_bytes() {
        let expected = crate::checksum::tests::REFERENCE_SUPERBLOCK;
        assert_eq!(superblock_v2(0x30, 0x0318_72b1), expected);
    }

    #[test]
    fn btree_levels_follow_the_node_capacity() {
        assert_eq!(btree_level_widths(0), vec![1]);
        assert_eq!(btree_level_widths(1), vec![1]);
        assert_eq!(btree_level_widths(64), vec![1]);
        assert_eq!(btree_level_widths(65), vec![1, 2]);
        assert_eq!(btree_level_widths(64 * 64), vec![1, 64]);
        assert_eq!(btree_level_widths(64 * 64 + 1), vec![1, 2, 65]);
    }

    #[test]
    fn btree_node_size_is_fixed_by_rank() {
        // header 24 + 64 child addresses + 65 keys of (8 + (rank + 1) * 8)
        assert_eq!(btree_node_size(1), 24 + 64 * 8 + 65 * 24);
        assert_eq!(btree_node_size(3), 24 + 64 * 8 + 65 * 40);
        assert_eq!(btree_size(150, 3), 4 * btree_node_size(3));
    }

    #[test]
    fn single_leaf_tree_lists_chunks_in_order_with_a_closing_key() {
        let chunks = vec![
            ChunkEntry { offsets: vec![0, 0], address: 100, size: 10 },
            ChunkEntry { offsets: vec![1, 0], address: 110, size: 12 },
        ];
        let tree = btree_v1(&chunks, &[1, 4], 5000);
        assert_eq!(tree.len(), btree_node_size(2) as usize);
        assert_eq!(&tree[..4], b"TREE");
        assert_eq!(tree[4], 1, "chunk node type");
        assert_eq!(tree[5], 0, "leaf level");
        assert_eq!(u16::from_le_bytes([tree[6], tree[7]]), 2);
        let key0 = &tree[24..24 + 32];
        assert_eq!(u32::from_le_bytes(key0[..4].try_into().unwrap()), 10);
        let child0 = &tree[56..64];
        assert_eq!(u64::from_le_bytes(child0.try_into().unwrap()), 100);
        let key2 = &tree[24 + 2 * 40..24 + 2 * 40 + 32];
        assert_eq!(u64::from_le_bytes(key2[8..16].try_into().unwrap()), 2);
        assert_eq!(u64::from_le_bytes(key2[16..24].try_into().unwrap()), 4);
    }

    #[test]
    fn object_header_carries_a_checksum_over_everything_before_it() {
        let header = object_header_v2(&[Message { type_id: MSG_GROUP_INFO, payload: group_info() }]);
        let body_len = header.len() - 4;
        let checksum = u32::from_le_bytes(header[body_len..].try_into().unwrap());
        assert_eq!(checksum, jenkins_lookup3(&header[..body_len]));
        assert_eq!(&header[..4], b"OHDR");
    }

    #[test]
    fn global_heap_collection_has_the_minimum_size_and_a_free_object() {
        let heap = global_heap_collection(&[vec![1; 8], vec![2; 8]]);
        assert_eq!(heap.len(), 4096);
        assert_eq!(&heap[..4], b"GCOL");
        assert_eq!(u64::from_le_bytes(heap[8..16].try_into().unwrap()), 4096);
        assert_eq!(u16::from_le_bytes([heap[16], heap[17]]), 1);
        assert_eq!(u16::from_le_bytes([heap[40], heap[41]]), 2);
    }
}
