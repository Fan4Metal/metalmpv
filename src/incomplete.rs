//! Whether a file whose duration may be an estimate (no index: an MP3, raw
//! AAC, …) lacks data, so that an early end of it is the data's and not
//! the file's: room a download keeps for data not there yet (zeros), or
//! fewer bytes than its header declares (an MP3's Xing or VBRI header,
//! else its ID3 length at its first frame's bitrate; a WAV's RIFF size).
//! A whole file whose duration was over-estimated from its first frame's
//! bitrate has neither.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Read at most this much to find a header or zeros.
const PROBE: u64 = 64 * 1024;

/// Whether the bytes of local file `path` at `at` of its `duration`
/// seconds (in proportion to its size) are all zeros: room a download
/// keeps for data not there yet (a torrent's file, made at its full size
/// at once). Real sound is never 64 KB of zeros: an MP3's frames, even
/// silent, start with a header every few hundred bytes.
pub fn unfilled(path: &str, at: f64, duration: f64) -> bool {
    if crate::playlist::is_url(Path::new(path)) || duration.is_nan() || duration <= 0.0 {
        return false;
    }
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let Ok(size) = file.metadata().map(|m| m.len()) else {
        return false;
    };
    let share = (at / duration).clamp(0.0, 1.0);
    let from = ((size as f64 * share) as u64).min(size.saturating_sub(PROBE));
    let mut bytes = Vec::new();
    file.seek(SeekFrom::Start(from)).is_ok()
        && file.take(PROBE).read_to_end(&mut bytes).is_ok()
        && !bytes.is_empty()
        && bytes.iter().all(|b| *b == 0)
}

/// Whether local file `path`, of `duration` seconds as mpv says, is
/// shorter than its header declares: a download cut short, whose
/// duration the header still gives whole (an MP3's Xing header, or its
/// ID3 length at its bitrate; a WAV's size). `mp3`: mpv's `file-format`
/// says so; the bytes of other formats (AAC, AC3) are not searched for an
/// MP3 frame, which their data has by chance.
pub fn truncated(path: &str, duration: f64, mp3: bool) -> bool {
    if crate::playlist::is_url(Path::new(path)) {
        return false;
    }
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let Ok(size) = file.metadata().map(|m| m.len()) else {
        return false;
    };
    // An ID3v2 tag first (with a cover, it can be large): ten bytes, then
    // its size in seven bits a byte, and a footer of ten more when its
    // flag says so.
    let mut tag = [0u8; 10];
    let start = match file.read_exact(&mut tag) {
        Ok(()) if tag.starts_with(b"ID3") => {
            let size = tag[6..10]
                .iter()
                .fold(0u64, |n, b| (n << 7) | u64::from(b & 0x7F));
            10 + size + if tag[5] & 0x10 != 0 { 10 } else { 0 }
        }
        Ok(()) => 0,
        Err(_) => return false,
    };
    let mut head = Vec::new();
    if file.seek(SeekFrom::Start(start)).is_err()
        || file.take(PROBE).read_to_end(&mut head).is_err()
    {
        return false;
    }
    declared(&head, duration, mp3).is_some_and(|declared| {
        size.saturating_add((declared / 50).max(4096)) < start.saturating_add(declared)
    })
}

/// The size `head` (a file's start after its ID3v2 tag) declares for
/// what follows: a WAV's RIFF size; an MP3's from its Xing (`Info` for
/// CBR) or VBRI header, else its first frame's bitrate over `duration`
/// (from its ID3 length; over-estimated from that bitrate, it gives the
/// size there is).
fn declared(head: &[u8], duration: f64, mp3: bool) -> Option<u64> {
    let be32 = |at: usize| -> Option<u64> {
        let b = head.get(at..at + 4)?;
        Some(u64::from(u32::from_be_bytes([b[0], b[1], b[2], b[3]])))
    };
    if head.len() >= 12 && head.starts_with(b"RIFF") && &head[8..12] == b"WAVE" {
        let size = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
        // Written by a stream that did not know its end.
        return (size != 0 && size != u32::MAX).then_some(u64::from(size) + 8);
    }
    if !mp3 {
        return None;
    }
    // The first frame of MPEG audio layer III.
    let frame = (0..head.len().saturating_sub(4)).find(|&i| {
        let h = &head[i..i + 4];
        h[0] == 0xFF
            && h[1] & 0xE0 == 0xE0
            && (h[1] >> 3) & 3 != 1 // a version
            && (h[1] >> 1) & 3 == 1 // layer III
            && !matches!(h[2] >> 4, 0 | 15) // a bitrate
            && (h[2] >> 2) & 3 != 3 // a sample rate
    })?;
    let mpeg1 = (head[frame + 1] >> 3) & 3 == 3;
    let mono = head[frame + 3] >> 6 == 3;
    let side = match (mpeg1, mono) {
        (true, true) => 17,
        (true, false) => 32,
        (false, true) => 9,
        (false, false) => 17,
    };
    let xing = frame + 4 + side;
    let tag = head.get(xing..xing + 4);
    if tag == Some(b"Xing") || tag == Some(b"Info") {
        let flags = be32(xing + 4)?;
        // The frame count first, when there is one.
        let bytes_at = xing + 8 + if flags & 1 != 0 { 4 } else { 0 };
        return (flags & 2 != 0)
            .then(|| be32(bytes_at))
            .flatten()
            .map(|bytes| frame as u64 + bytes);
    }
    let vbri = frame + 4 + 32;
    if head.get(vbri..vbri + 4) == Some(b"VBRI") {
        return be32(vbri + 10).map(|bytes| frame as u64 + bytes);
    }
    const MPEG1: [u64; 15] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
    ];
    const MPEG2: [u64; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
    let kbps = if mpeg1 { MPEG1 } else { MPEG2 }[usize::from(head[frame + 2] >> 4)];
    // A frame header found by chance has no other at its frame's length.
    let rate = match (head[frame + 1] >> 3) & 3 {
        3 => [44_100, 48_000, 32_000],
        2 => [22_050, 24_000, 16_000],
        _ => [11_025, 12_000, 8_000],
    }[usize::from((head[frame + 2] >> 2) & 3)];
    let padding = u64::from((head[frame + 2] >> 1) & 1);
    let length = (if mpeg1 { 144 } else { 72 } * kbps * 1000 / rate + padding) as usize;
    let next = head.get(frame + length..frame + length + 2)?;
    if next[0] != 0xFF || next[1] & 0xE0 != 0xE0 {
        return None;
    }
    (duration.is_finite() && duration > 0.0)
        .then(|| (frame as u64).saturating_add((duration * kbps as f64 * 125.0) as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An MP3's start: an ID3v2 tag of `tag` bytes, then a MPEG-1 stereo
    /// frame of 128 kb/s (417 bytes), with a Xing header declaring `bytes`
    /// if any, and the next frame's header.
    fn mp3_head(tag: usize, bytes: Option<u32>) -> Vec<u8> {
        let mut head = b"ID3\x04\x00\x00".to_vec();
        head.extend([0, 0, (tag >> 7) as u8 & 0x7F, tag as u8 & 0x7F]);
        head.extend(vec![0; tag]);
        // 128 kb/s, 44.1 kHz, stereo.
        let frame = head.len();
        head.extend([0xFF, 0xFB, 0x90, 0x00]);
        head.extend([0; 32]);
        if let Some(bytes) = bytes {
            head.extend(b"Xing");
            head.extend(3u32.to_be_bytes());
            head.extend(1000u32.to_be_bytes());
            head.extend(bytes.to_be_bytes());
        }
        head.resize(frame + 417, 0);
        head.extend([0xFF, 0xFB, 0x90, 0x00]);
        head.extend([0; 300]);
        head
    }

    #[test]
    fn headers_declare_sizes() {
        let after_tag = |head: Vec<u8>| head[210..].to_vec();
        assert_eq!(
            declared(&after_tag(mp3_head(200, Some(500_000))), 0.0, true),
            Some(500_000)
        );
        // No Xing header: 128 kb/s for 60 s.
        assert_eq!(
            declared(&after_tag(mp3_head(200, None)), 60.0, true),
            Some(960_000)
        );
        assert_eq!(declared(&after_tag(mp3_head(200, None)), 0.0, true), None);
        let mut wav = b"RIFF".to_vec();
        wav.extend(1_000u32.to_le_bytes());
        wav.extend(b"WAVEfmt ");
        assert_eq!(declared(&wav, 0.0, false), Some(1_008));
        let mut stream = b"RIFF".to_vec();
        stream.extend(u32::MAX.to_le_bytes());
        stream.extend(b"WAVE");
        assert_eq!(declared(&stream, 0.0, false), None);
        assert_eq!(declared(b"not audio", 60.0, true), None);
        // Not an MP3 (AAC): its bytes are not searched for a frame.
        assert_eq!(declared(&after_tag(mp3_head(200, None)), 60.0, false), None);
        // A frame header by chance, without the next one.
        let mut chance = vec![0x12u8; 50];
        chance.extend([0xFF, 0xFB, 0x90, 0x00]);
        chance.extend(vec![0x34u8; 1000]);
        assert_eq!(declared(&chance, 60.0, true), None);
    }

    #[test]
    fn short_files_and_zeros() {
        let dir = std::env::temp_dir().join(format!("incomplete_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = |name: &str| dir.join(name).to_string_lossy().into_owned();
        // Declares 500 KB, has some 4 KB: cut short.
        std::fs::write(path("cut.mp3"), mp3_head(200, Some(500_000))).unwrap();
        assert!(truncated(&path("cut.mp3"), 30.0, true));
        // Declares what it has.
        let size = mp3_head(200, Some(0)).len() as u32 - 210;
        std::fs::write(path("whole.mp3"), mp3_head(200, Some(size))).unwrap();
        assert!(!truncated(&path("whole.mp3"), 30.0, true));
        // No Xing header: 60 s at 128 kb/s are some 940 KB, not 0.5 KB;
        // a duration estimated from the size gives the size.
        std::fs::write(path("cbr.mp3"), mp3_head(200, None)).unwrap();
        assert!(truncated(&path("cbr.mp3"), 60.0, true));
        let estimated = (mp3_head(200, None).len() - 210) as f64 * 8.0 / 128_000.0;
        assert!(!truncated(&path("cbr.mp3"), estimated, true));
        // Data in the first half, zeros in the second.
        let mut half = vec![0xFFu8; 512 * 1024];
        half.extend(vec![0u8; 512 * 1024]);
        std::fs::write(path("half.mp3"), &half).unwrap();
        assert!(!unfilled(&path("half.mp3"), 10.0, 100.0));
        assert!(unfilled(&path("half.mp3"), 60.0, 100.0));
        assert!(unfilled(&path("half.mp3"), 100.0, 100.0));
        assert!(!unfilled("https://example.com/a.mp3", 60.0, 100.0));
        assert!(!unfilled(&path("half.mp3"), 60.0, 0.0));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
