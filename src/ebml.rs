//! Just enough of EBML, Matroska's encoding, to find the Segment's
//! top-level elements and read their children: `cover.rs` takes the
//! attachments, `probe.rs` the information, the tracks and the tags. The
//! SeekHead says where an element is (the tags and the attachments may come
//! after the clusters), else the Segment's children are walked up to the
//! first cluster.

use std::io::{Read, Seek, SeekFrom};

pub const EBML: u32 = 0x1A45_DFA3;
pub const SEGMENT: u32 = 0x1853_8067;
const SEEK_HEAD: u32 = 0x114D_9B74;
const SEEK: u32 = 0x4DBB;
const SEEK_ID: u32 = 0x53AB;
const SEEK_POSITION: u32 = 0x53AC;
const CLUSTER: u32 = 0x1F43_B675;

/// An element's id and the size of its data (`None`: unknown).
pub fn element(r: &mut impl Read) -> Option<(u32, Option<u64>)> {
    let (id, _) = vint(r, true)?;
    let (size, all_ones) = vint(r, false)?;
    Some((id as u32, (!all_ones).then_some(size)))
}

/// A variable-size integer; `marker` keeps its length marker (ids do).
/// Also whether all its value bits are set (an unknown size).
pub fn vint(r: &mut impl Read, marker: bool) -> Option<(u64, bool)> {
    let mut first = [0u8];
    r.read_exact(&mut first).ok()?;
    let len = first[0].leading_zeros() as usize + 1;
    if len > 8 {
        return None;
    }
    let mask = if len == 8 { 0 } else { 0xFFu8 >> len };
    let mut value = u64::from(if marker { first[0] } else { first[0] & mask });
    let mut all_ones = first[0] & mask == mask;
    for _ in 1..len {
        let mut b = [0u8];
        r.read_exact(&mut b).ok()?;
        value = (value << 8) | u64::from(b[0]);
        all_ones &= b[0] == 0xFF;
    }
    Some((value, all_ones))
}

/// `size` bytes, if no more than `limit`.
pub fn read_bytes(r: &mut impl Read, size: u64, limit: u64) -> Option<Vec<u8>> {
    if size > limit {
        return None;
    }
    let mut data = vec![0; size as usize];
    r.read_exact(&mut data).ok()?;
    Some(data)
}

/// An unsigned integer element's value.
pub fn read_uint(r: &mut (impl Read + Seek), size: u64) -> Option<u64> {
    if size > 8 {
        // Not a number: skipped, so the next element is read where it is.
        r.seek_relative(i64::try_from(size).ok()?).ok()?;
        return None;
    }
    let data = read_bytes(r, size, 8)?;
    Some(data.iter().fold(0, |v, b| (v << 8) | u64::from(*b)))
}

/// A signed integer element's value.
pub fn read_int(r: &mut (impl Read + Seek), size: u64) -> Option<i64> {
    let value = read_uint(r, size)?;
    if size == 0 {
        return Some(0);
    }
    let shift = 64 - 8 * size as u32;
    Some(((value << shift) as i64) >> shift)
}

/// A string element's value (UTF-8, or ASCII), without the NULs that may
/// pad it.
pub fn read_text(r: &mut impl Read, size: u64, limit: u64) -> Option<String> {
    let data = read_bytes(r, size, limit)?;
    let text = String::from_utf8_lossy(&data);
    Some(text.trim_end_matches('\0').trim().to_owned())
}

/// Where the SeekHead in `data` puts element `wanted`, from the Segment's
/// data.
fn seek_position(data: &[u8], wanted: u32) -> Option<u64> {
    let mut r = std::io::Cursor::new(data);
    while (r.position() as usize) < data.len() {
        let (id, size) = element(&mut r)?;
        let size = size?;
        if id != SEEK {
            r.seek_relative(size as i64).ok()?;
            continue;
        }
        let end = r.position() + size;
        let (mut seek_id, mut position) = (None, None);
        while r.position() < end {
            let (id, size) = element(&mut r)?;
            let size = size?;
            match id {
                SEEK_ID => seek_id = read_uint(&mut r, size),
                SEEK_POSITION => position = read_uint(&mut r, size),
                _ => r.seek_relative(size as i64).ok()?,
            }
        }
        if seek_id == Some(u64::from(wanted)) {
            return position;
        }
    }
    None
}

/// The Segment's top-level elements among `wanted` (the first of each):
/// the id, where its data starts and its size. `None` if the file is not
/// Matroska.
pub fn top_level(r: &mut (impl Read + Seek), wanted: &[u32]) -> Option<Vec<(u32, u64, u64)>> {
    r.seek(SeekFrom::Start(0)).ok()?;
    let (id, size) = element(r)?;
    if id != EBML {
        return None;
    }
    r.seek_relative(size? as i64).ok()?;
    let (id, segment_size) = element(r)?;
    if id != SEGMENT {
        return None;
    }
    let start = r.stream_position().ok()?;
    let end = segment_size.map(|s| start + s);
    let mut found: Vec<(u32, u64, u64)> = Vec::new();
    let mut sought: Vec<(u32, u64)> = Vec::new();
    let has = |found: &[(u32, u64, u64)], id: u32| found.iter().any(|(f, _, _)| *f == id);
    // The Segment's children up to the first cluster.
    for _ in 0..64 {
        let at = r.stream_position().ok()?;
        if end.is_some_and(|e| at >= e) || wanted.iter().all(|w| has(&found, *w)) {
            break;
        }
        let Some((id, Some(size))) = element(r) else {
            break;
        };
        let data = r.stream_position().ok()?;
        match id {
            CLUSTER => break,
            SEEK_HEAD => {
                let Some(bytes) = read_bytes(r, size, 1 << 20) else {
                    break;
                };
                for w in wanted {
                    if let Some(p) = seek_position(&bytes, *w) {
                        sought.push((*w, p));
                    }
                }
            }
            id => {
                if wanted.contains(&id) && !has(&found, id) {
                    found.push((id, data, size));
                }
                r.seek(SeekFrom::Start(data.checked_add(size)?)).ok()?;
            }
        }
    }
    // Those after the clusters, where the SeekHead says.
    for (id, position) in sought {
        if has(&found, id) {
            continue;
        }
        let Some(at) = start.checked_add(position) else {
            continue;
        };
        if r.seek(SeekFrom::Start(at)).is_err() {
            continue;
        }
        if let Some((got, Some(size))) = element(r)
            && got == id
        {
            found.push((id, r.stream_position().ok()?, size));
        }
    }
    Some(found)
}

/// The children of an element whose data runs from where `r` is for
/// `size` bytes: each one's id and size, `r` left at its data; `each`
/// reads it or leaves it (it is skipped when `r` was not moved).
pub fn children<R: Read + Seek>(
    r: &mut R,
    size: u64,
    mut each: impl FnMut(&mut R, u32, u64) -> Option<()>,
) -> Option<()> {
    let end = r.stream_position().ok()?.checked_add(size)?;
    while r.stream_position().ok()? < end {
        let (id, size) = element(r)?;
        let size = size?;
        let data = r.stream_position().ok()?;
        each(r, id, size)?;
        r.seek(SeekFrom::Start(data.checked_add(size)?)).ok()?;
    }
    Some(())
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// An EBML element with a size of one or two bytes.
    pub fn el(id: &[u8], data: &[u8]) -> Vec<u8> {
        let mut v = id.to_vec();
        if data.len() < 127 {
            v.push(0x80 | data.len() as u8);
        } else {
            assert!(data.len() < 0x3FFF);
            v.extend([0x40 | (data.len() >> 8) as u8, data.len() as u8]);
        }
        v.extend_from_slice(data);
        v
    }

    #[test]
    fn vints() {
        let mut r = std::io::Cursor::new([0x81u8, 0x40, 0x02, 0xFF, 0x1A, 0x45, 0xDF, 0xA3]);
        assert_eq!(vint(&mut r, false), Some((1, false)));
        assert_eq!(vint(&mut r, false), Some((2, false)));
        assert_eq!(vint(&mut r, false), Some((0x7F, true)));
        assert_eq!(vint(&mut r, true), Some((u64::from(EBML), false)));
    }

    #[test]
    fn signed() {
        let mut r = std::io::Cursor::new([0xFFu8, 0xFE, 0x01, 0x00]);
        assert_eq!(read_int(&mut r, 2), Some(-2));
        assert_eq!(read_int(&mut r, 2), Some(256));
    }
}
