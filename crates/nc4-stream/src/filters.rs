//! The two HDF5 filters NetCDF-4 output uses: byte shuffle and deflate.

use std::io::Write;

use flate2::{write::ZlibEncoder, Compression};

/// HDF5 shuffle filter (id 2): transpose the bytes of every element so that
/// all first bytes come first, then all second bytes, and so on. Deflate
/// compresses the result much better for floating-point grids.
pub(crate) fn shuffle(data: &[u8], element_size: usize) -> Vec<u8> {
    if element_size <= 1 || data.len() < element_size {
        return data.to_vec();
    }
    let elements = data.len() / element_size;
    let mut out = vec![0u8; data.len()];
    for byte in 0..element_size {
        let destination = &mut out[byte * elements..(byte + 1) * elements];
        for (element, slot) in destination.iter_mut().enumerate() {
            *slot = data[element * element_size + byte];
        }
    }
    let complete = elements * element_size;
    out[complete..].copy_from_slice(&data[complete..]);
    out
}

/// HDF5 deflate filter (id 1): a zlib stream at the given level, 1 to 9.
pub(crate) fn deflate(data: &[u8], level: u32) -> std::io::Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(
        Vec::with_capacity(data.len() / 2),
        Compression::new(level),
    );
    encoder.write_all(data)?;
    encoder.finish()
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::{deflate, shuffle};

    #[test]
    fn shuffle_groups_bytes_by_position() {
        let data = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        assert_eq!(shuffle(&data, 4), [1, 5, 9, 2, 6, 10, 3, 7, 11, 4, 8, 12]);
        assert_eq!(shuffle(&data, 1), data);
    }

    #[test]
    fn shuffle_leaves_a_trailing_partial_element_in_place() {
        let data = [1u8, 2, 3, 4, 5, 6, 7];
        assert_eq!(shuffle(&data, 2), [1, 3, 5, 2, 4, 6, 7]);
    }

    #[test]
    fn deflate_produces_a_zlib_stream_that_inflates_back() {
        let data: Vec<u8> = (0..4096).map(|i| (i % 17) as u8).collect();
        let packed = deflate(&data, 6).unwrap();
        assert!(packed.len() < data.len() / 4);
        assert_eq!(packed[0] & 0x0f, 8, "zlib header with deflate method");
        let mut unpacked = Vec::new();
        flate2::read::ZlibDecoder::new(packed.as_slice())
            .read_to_end(&mut unpacked)
            .unwrap();
        assert_eq!(unpacked, data);
    }
}
