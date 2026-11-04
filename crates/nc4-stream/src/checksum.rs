//! Jenkins lookup3 hash as HDF5 uses it for metadata checksums
//! (`H5_checksum_lookup3` with an initial value of zero).

fn mix(a: &mut u32, b: &mut u32, c: &mut u32) {
    *a = a.wrapping_sub(*c);
    *a ^= c.rotate_left(4);
    *c = c.wrapping_add(*b);
    *b = b.wrapping_sub(*a);
    *b ^= a.rotate_left(6);
    *a = a.wrapping_add(*c);
    *c = c.wrapping_sub(*b);
    *c ^= b.rotate_left(8);
    *b = b.wrapping_add(*a);
    *a = a.wrapping_sub(*c);
    *a ^= c.rotate_left(16);
    *c = c.wrapping_add(*b);
    *b = b.wrapping_sub(*a);
    *b ^= a.rotate_left(19);
    *a = a.wrapping_add(*c);
    *c = c.wrapping_sub(*b);
    *c ^= b.rotate_left(4);
    *b = b.wrapping_add(*a);
}

fn final_mix(a: &mut u32, b: &mut u32, c: &mut u32) {
    *c ^= *b;
    *c = c.wrapping_sub(b.rotate_left(14));
    *a ^= *c;
    *a = a.wrapping_sub(c.rotate_left(11));
    *b ^= *a;
    *b = b.wrapping_sub(a.rotate_left(25));
    *c ^= *b;
    *c = c.wrapping_sub(b.rotate_left(16));
    *a ^= *c;
    *a = a.wrapping_sub(c.rotate_left(4));
    *b ^= *a;
    *b = b.wrapping_sub(a.rotate_left(14));
    *c ^= *b;
    *c = c.wrapping_sub(b.rotate_left(24));
}

fn word(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Checksum of `data`, as stored after HDF5 superblocks and object headers.
pub(crate) fn jenkins_lookup3(data: &[u8]) -> u32 {
    let mut a = 0xdead_beef_u32.wrapping_add(data.len() as u32);
    let mut b = a;
    let mut c = a;

    let mut rest = data;
    while rest.len() > 12 {
        a = a.wrapping_add(word(&rest[0..4]));
        b = b.wrapping_add(word(&rest[4..8]));
        c = c.wrapping_add(word(&rest[8..12]));
        mix(&mut a, &mut b, &mut c);
        rest = &rest[12..];
    }

    if rest.is_empty() {
        return c;
    }
    let mut tail = [0u8; 12];
    tail[..rest.len()].copy_from_slice(rest);
    a = a.wrapping_add(word(&tail[0..4]));
    b = b.wrapping_add(word(&tail[4..8]));
    c = c.wrapping_add(word(&tail[8..12]));
    final_mix(&mut a, &mut b, &mut c);
    c
}

#[cfg(test)]
pub(crate) mod tests {
    use super::jenkins_lookup3;

    /// The first 48 bytes of a file written by netcdf-c 4.9.2 / HDF5 1.10.7:
    /// a version 2 superblock whose last four bytes are its checksum.
    pub(crate) const REFERENCE_SUPERBLOCK: [u8; 48] = [
        0x89, 0x48, 0x44, 0x46, 0x0d, 0x0a, 0x1a, 0x0a, 0x02, 0x08, 0x08, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xb1, 0x72,
        0x18, 0x03, 0x00, 0x00, 0x00, 0x00, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x8d,
        0x2c, 0x64, 0x55,
    ];

    #[test]
    fn matches_the_checksum_libhdf5_wrote() {
        let checksum = jenkins_lookup3(&REFERENCE_SUPERBLOCK[..44]);
        assert_eq!(checksum.to_le_bytes(), REFERENCE_SUPERBLOCK[44..]);
    }

    #[test]
    fn empty_input_is_the_seeded_constant() {
        assert_eq!(jenkins_lookup3(&[]), 0xdead_beef);
    }

    #[test]
    fn block_boundaries_do_not_change_the_tail_handling() {
        // 12, 13 and 24 bytes exercise the "exactly one block", "block plus
        // one" and "two blocks" paths; they must all differ and be stable.
        let data: Vec<u8> = (0..24).collect();
        let twelve = jenkins_lookup3(&data[..12]);
        let thirteen = jenkins_lookup3(&data[..13]);
        let twenty_four = jenkins_lookup3(&data);
        assert_ne!(twelve, thirteen);
        assert_ne!(thirteen, twenty_four);
        assert_eq!(twelve, jenkins_lookup3(&data[..12]));
    }
}
