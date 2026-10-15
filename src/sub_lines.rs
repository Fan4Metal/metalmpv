//! When each line of a subtitle file shows, for the timeline of the
//! panel's Subtitles tab, where the delay is matched to the speech by
//! sight. Only subtitle files of their own (SRT, WebVTT, ASS/SSA): mpv
//! gives the line on screen and no more, and the subtitles of a film's
//! own file are seldom out of step. Only the times are read, so the text's
//! encoding does not matter (UTF-16 is turned into text first, as its
//! digits are two bytes).

use std::path::Path;

/// A line's span, in seconds of the file's own time (before mpv's delay).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub start: f64,
    pub end: f64,
}

/// Files larger than this are not read (a subtitle file is a few hundred
/// kilobytes; karaoke ASS a few megabytes).
const MAX_SIZE: u64 = 32 * 1024 * 1024;

/// The lines of the subtitle file `path`, by their start; `None` for a
/// format not read here, a file that cannot be read, or one without lines.
pub fn read(path: &Path) -> Option<Vec<Line>> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if !matches!(ext.as_str(), "srt" | "vtt" | "ass" | "ssa") {
        return None;
    }
    if std::fs::metadata(path).ok()?.len() > MAX_SIZE {
        return None;
    }
    let text = decode(&std::fs::read(path).ok()?);
    let mut lines = match ext.as_str() {
        "ass" | "ssa" => ass_lines(&text),
        _ => arrow_lines(&text),
    };
    lines.sort_by(|a, b| a.start.total_cmp(&b.start));
    (!lines.is_empty()).then_some(lines)
}

/// The text of a file: UTF-16 by its byte order mark, else bytes as UTF-8
/// (the digits and the punctuation of the times are ASCII in any 8-bit
/// code page).
fn decode(bytes: &[u8]) -> String {
    let utf16 = |be: bool| {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| {
                if be {
                    u16::from_be_bytes(c)
                } else {
                    u16::from_le_bytes(c)
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xFF, 0xFE, ..] => utf16(false),
        [0xFE, 0xFF, ..] => utf16(true),
        // UTF-8's mark before a time line would hide that line.
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// SRT and WebVTT: a line `start --> end` (WebVTT may add settings after
/// the end) before each cue's text.
fn arrow_lines(text: &str) -> Vec<Line> {
    text.lines()
        .filter_map(|line| {
            let (start, rest) = line.split_once("-->")?;
            let end = rest.split_whitespace().next()?;
            let (start, end) = (time(start.trim())?, time(end)?);
            (end >= start).then_some(Line { start, end })
        })
        .collect()
}

/// ASS/SSA: the `Dialogue:` lines of `[Events]`, their start and end where
/// the section's `Format:` line puts them (else the standard places).
fn ass_lines(text: &str) -> Vec<Line> {
    let mut events = false;
    let (mut start_at, mut end_at) = (1, 2);
    let mut lines = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            events = line.eq_ignore_ascii_case("[events]");
            continue;
        }
        if !events {
            continue;
        }
        if let Some(format) = line.strip_prefix("Format:") {
            let fields: Vec<String> = format
                .split(',')
                .map(|f| f.trim().to_ascii_lowercase())
                .collect();
            if let (Some(s), Some(e)) = (
                fields.iter().position(|f| f == "start"),
                fields.iter().position(|f| f == "end"),
            ) {
                (start_at, end_at) = (s, e);
            }
        } else if let Some(dialogue) = line.strip_prefix("Dialogue:") {
            // The text, last, may hold commas: only the fields before it.
            let fields: Vec<&str> = dialogue.splitn(start_at.max(end_at) + 2, ',').collect();
            let field = |i: usize| fields.get(i).and_then(|f| time(f.trim()));
            if let (Some(start), Some(end)) = (field(start_at), field(end_at))
                && end >= start
            {
                lines.push(Line { start, end });
            }
        }
    }
    lines
}

/// `H:MM:SS,mmm`, `HH:MM:SS.mmm`, `MM:SS.mmm` or `H:MM:SS.cc` in seconds.
fn time(text: &str) -> Option<f64> {
    let parts: Vec<&str> = text.split(':').collect();
    let (h, m, s) = match parts[..] {
        [h, m, s] => (h.parse::<u32>().ok()?, m, s),
        [m, s] => (0, m, s),
        _ => return None,
    };
    let m = m.parse::<u32>().ok()?;
    let s = s.replace(',', ".").parse::<f64>().ok()?;
    let t = f64::from(h) * 3600.0 + f64::from(m) * 60.0 + s;
    (t.is_finite() && t >= 0.0).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(time("00:01:02,500"), Some(62.5));
        assert_eq!(time("1:00:00.25"), Some(3600.25));
        assert_eq!(time("01:02.000"), Some(62.0));
        assert_eq!(time("x:01:02"), None);
        assert_eq!(time("12"), None);
    }

    #[test]
    fn srt_and_vtt() {
        let srt = "1\r\n00:00:01,000 --> 00:00:02,500\r\nHello\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nWorld, again\r\n";
        assert_eq!(
            arrow_lines(srt),
            [
                Line {
                    start: 1.0,
                    end: 2.5
                },
                Line {
                    start: 3.0,
                    end: 4.0
                }
            ]
        );
        let vtt = "WEBVTT\n\n00:05.000 --> 00:07.000 align:start position:10%\n- Hi\n";
        assert_eq!(
            arrow_lines(vtt),
            [Line {
                start: 5.0,
                end: 7.0
            }]
        );
    }

    #[test]
    fn ass_by_its_format() {
        let ass = "[Script Info]\nTitle: x\n\n[V4+ Styles]\nFormat: Name, Fontname\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nComment: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,note\nDialogue: 0,0:00:01.50,0:00:03.00,Default,,0,0,0,,Hi, there, all\nDialogue: 0,0:01:00.00,0:01:02.25,Default,,0,0,0,,{\\i1}Bye\n";
        assert_eq!(
            ass_lines(ass),
            [
                Line {
                    start: 1.5,
                    end: 3.0
                },
                Line {
                    start: 60.0,
                    end: 62.25
                }
            ]
        );
        // Another order of the fields.
        let odd = "[Events]\nFormat: Start, End, Text\nDialogue: 0:00:02.00,0:00:04.00,Text\n";
        assert_eq!(
            ass_lines(odd),
            [Line {
                start: 2.0,
                end: 4.0
            }]
        );
    }

    #[test]
    fn utf16_files() {
        let text = "00:00:01,000 --> 00:00:02,000\r\nПривет\r\n";
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(arrow_lines(&decode(&bytes)).len(), 1);
        let mut bytes = vec![0xFE, 0xFF];
        bytes.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
        assert_eq!(decode(&bytes), text);
        // UTF-8 with its mark, the first line a time line.
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend(text.as_bytes());
        assert_eq!(decode(&bytes), text);
        assert_eq!(arrow_lines(&decode(&bytes)).len(), 1);
    }
}
