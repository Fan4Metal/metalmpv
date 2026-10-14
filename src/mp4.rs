//! Just enough of MP4's (ISO BMFF, QuickTime) boxes to walk down to one:
//! `cover.rs` reads the tags' cover, `probe.rs` the tracks' decoder
//! configuration. Read here, not with lofty: lofty 0.25 skips a box with a
//! 64-bit size 8 bytes too far, and a film over 4 GB has its mdat written
//! so, before the moov.

use std::io::{Read, Seek, SeekFrom};

/// A box's type and the size of its data, the reader left at its data; a
/// box of size 0 reaches `end`.
pub fn next_box(r: &mut (impl Read + Seek), end: u64) -> Option<([u8; 4], u64)> {
    let at = r.stream_position().ok()?;
    if at + 8 > end {
        return None;
    }
    let mut header = [0u8; 8];
    r.read_exact(&mut header).ok()?;
    let kind = [header[4], header[5], header[6], header[7]];
    let (size, header_len) = match u32::from_be_bytes([header[0], header[1], header[2], header[3]])
    {
        0 => (end - at, 8),
        1 => {
            let mut size = [0u8; 8];
            r.read_exact(&mut size).ok()?;
            (u64::from_be_bytes(size), 16)
        }
        size => (u64::from(size), 8),
    };
    (size >= header_len && at.checked_add(size)? <= end).then_some((kind, size - header_len))
}

/// The end of the first box `kind` among the boxes up to `end`, the reader
/// left at its data.
pub fn child(r: &mut (impl Read + Seek), end: u64, kind: &[u8; 4]) -> Option<u64> {
    loop {
        let (found, size) = next_box(r, end)?;
        let data = r.stream_position().ok()?;
        if &found == kind {
            return Some(data + size);
        }
        r.seek(SeekFrom::Start(data + size)).ok()?;
    }
}

/// The path of boxes `kinds` down from where `r` is: the end of the last,
/// the reader left at its data.
pub fn path(r: &mut (impl Read + Seek), end: u64, kinds: &[&[u8; 4]]) -> Option<u64> {
    kinds.iter().try_fold(end, |end, kind| child(r, end, kind))
}

/// Each box up to `end`: its type and the end of its data, `r` left at
/// its data; `each` reads it or leaves it.
pub fn each_box<R: Read + Seek>(
    r: &mut R,
    end: u64,
    mut each: impl FnMut(&mut R, [u8; 4], u64) -> Option<()>,
) -> Option<()> {
    while let Some((kind, size)) = next_box(r, end) {
        let data = r.stream_position().ok()?;
        each(r, kind, data + size)?;
        r.seek(SeekFrom::Start(data + size)).ok()?;
    }
    Some(())
}

#[cfg(test)]
pub mod tests {
    /// An MP4 box.
    pub fn bx(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v = ((data.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend(kind);
        v.extend_from_slice(data);
        v
    }
}
