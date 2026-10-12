//! What the file's information window says (`ui/info.rs`): the tags in a
//! familiar order with their names in the interface's language, the file's
//! size, container and bitrate, each track's details, and all of it as
//! plain text for the clipboard. Each function has an `_in(lang, …)` form,
//! as in `format.rs`.

use std::path::Path;

use crate::i18n::Lang;
use crate::player::{State, VideoParams};
use crate::tracks::{self, Kind, Track};

/// The tags shown first, in this order: mpv's names (any case) and the
/// label in English and Russian.
const KNOWN_TAGS: &[(&[&str], &str, &str)] = &[
    (&["title"], "Title", "Название"),
    (&["artist"], "Artist", "Исполнитель"),
    (
        &["album_artist", "albumartist", "album artist"],
        "Album artist",
        "Исполнитель альбома",
    ),
    (&["album"], "Album", "Альбом"),
    (&["track", "tracknumber"], "Track", "Дорожка"),
    (&["disc", "discnumber"], "Disc", "Диск"),
    (&["date", "year", "originaldate"], "Year", "Год"),
    (&["genre"], "Genre", "Жанр"),
    (&["composer"], "Composer", "Композитор"),
    (&["performer"], "Performer", "Исполнитель (партия)"),
    (&["conductor"], "Conductor", "Дирижёр"),
    (
        &["publisher", "label", "organization"],
        "Publisher",
        "Издатель",
    ),
    (&["copyright"], "Copyright", "Авторские права"),
    (&["comment", "description"], "Comment", "Комментарий"),
    (&["language"], "Language", "Язык"),
    (
        &["encoder", "encoded_by", "encoded-by", "encodedby"],
        "Encoded with",
        "Кодировщик",
    ),
    (
        &["creation_time", "date_released", "date_recorded"],
        "Created",
        "Создан",
    ),
];

/// The value of the first of `keys` among the tags (names in any case).
pub fn tag<'a>(metadata: &'a [(String, String)], keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| {
        metadata
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    })
}

/// The tags as label and value: the known ones first, in their order and
/// named in `lang`, then the rest under the file's own names.
pub fn tags_in(lang: Lang, metadata: &[(String, String)]) -> Vec<(String, String)> {
    let mut used = vec![false; metadata.len()];
    let mut out = Vec::new();
    for (keys, en, ru) in KNOWN_TAGS {
        for key in *keys {
            if let Some(i) = metadata
                .iter()
                .position(|(k, _)| k.eq_ignore_ascii_case(key))
                .filter(|i| !used[*i])
            {
                used[i] = true;
                let label = if lang == Lang::Ru { ru } else { en };
                out.push(((*label).to_owned(), metadata[i].1.clone()));
                break;
            }
        }
    }
    out.extend(
        metadata
            .iter()
            .zip(used)
            .filter(|(_, used)| !used)
            .map(|((k, v), _)| (k.clone(), v.clone())),
    );
    out
}

/// The artist for the header: the artist, else the album's.
pub fn artist(metadata: &[(String, String)]) -> Option<&str> {
    tag(
        metadata,
        &["artist", "album_artist", "albumartist", "performer"],
    )
}

/// The album with its year: "Paranoid (1970)".
pub fn album(metadata: &[(String, String)]) -> Option<String> {
    let album = tag(metadata, &["album"])?;
    // The year of a full date ("1970-09-18").
    let year = tag(metadata, &["date", "year", "originaldate"])
        .map(|d| d.chars().take(4).collect::<String>())
        .filter(|y| y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()));
    Some(match year {
        Some(y) => format!("{album} ({y})"),
        None => album.to_owned(),
    })
}

/// A number with a decimal comma in Russian.
fn decimal_in(lang: Lang, value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    match lang {
        Lang::En => text,
        Lang::Ru => text.replace('.', ","),
    }
}

/// A file size: "734 KB", "1.37 GB"; "1,37 ГБ".
pub fn size_in(lang: Lang, bytes: i64) -> String {
    let units: [&str; 5] = match lang {
        Lang::En => ["bytes", "KB", "MB", "GB", "TB"],
        Lang::Ru => ["байт", "КБ", "МБ", "ГБ", "ТБ"],
    };
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < units.len() {
        value /= 1024.0;
        unit += 1;
    }
    let decimals = match unit {
        0 => 0,
        _ if value >= 100.0 => 0,
        _ if value >= 10.0 => 1,
        _ => 2,
    };
    format!("{} {}", decimal_in(lang, value, decimals), units[unit])
}

/// A bitrate: "320 kb/s", "8.5 Mb/s"; "8,5 Мбит/с".
pub fn bitrate_in(lang: Lang, bits_per_second: f64) -> String {
    let (k, m) = match lang {
        Lang::En => ("kb/s", "Mb/s"),
        Lang::Ru => ("кбит/с", "Мбит/с"),
    };
    if bits_per_second >= 1_000_000.0 {
        let mb = bits_per_second / 1_000_000.0;
        let decimals = if mb >= 100.0 { 0 } else { 1 };
        format!("{} {m}", decimal_in(lang, mb, decimals))
    } else {
        format!("{:.0} {k}", bits_per_second / 1000.0)
    }
}

/// The container as people know it, from FFmpeg's demuxer name.
pub fn container(file_format: &str) -> String {
    match file_format {
        "matroska,webm" | "matroska" => "Matroska".into(),
        "webm" => "WebM".into(),
        f if f.starts_with("mov,mp4") => "MP4 / QuickTime".into(),
        "mpegts" => "MPEG-TS".into(),
        "mpeg" => "MPEG-PS".into(),
        "asf" => "ASF (WMV / WMA)".into(),
        "ogg" => "Ogg".into(),
        "flv" => "Flash Video".into(),
        f => f.split(',').next().unwrap_or(f).to_uppercase(),
    }
}

/// The kind of a track: "Video", "Аудио".
pub fn kind_in(lang: Lang, kind: Kind, albumart: bool) -> &'static str {
    match (kind, albumart, lang) {
        (Kind::Video, true, Lang::En) => "Cover",
        (Kind::Video, true, Lang::Ru) => "Обложка",
        (Kind::Video, _, Lang::En) => "Video",
        (Kind::Video, _, Lang::Ru) => "Видео",
        (Kind::Audio, _, Lang::En) => "Audio",
        (Kind::Audio, _, Lang::Ru) => "Аудио",
        (Kind::Sub, _, Lang::En) => "Subtitles",
        (Kind::Sub, _, Lang::Ru) => "Субтитры",
    }
}

/// A track's technical details in one line:
/// "HEVC · 3840×2160 · 23,976 fps · 10 бит · HDR (PQ) · 14,2 Мбит/с",
/// "AAC · 5.1 · 48 кГц · 384 кбит/с". `params` are the decoded picture's,
/// for the selected video track.
pub fn track_details_in(lang: Lang, track: &Track, params: Option<&VideoParams>) -> String {
    let mut parts = Vec::new();
    if let Some(codec) = &track.codec {
        parts.push(tracks::codec_name(codec));
    }
    match track.kind {
        Kind::Video => {
            if let (Some(w), Some(h)) = (track.width, track.height) {
                parts.push(format!("{w}×{h}"));
            }
            if let Some(fps) = track.fps.filter(|f| *f > 0.0 && !track.albumart) {
                let decimals = if (fps - fps.round()).abs() < 0.001 {
                    0
                } else {
                    3
                };
                parts.push(format!("{} fps", decimal_in(lang, fps, decimals)));
            }
            if let Some(params) = params {
                if let Some(bits) = params.pixel_format.as_deref().and_then(bit_depth) {
                    parts.push(match lang {
                        Lang::En => format!("{bits}-bit"),
                        Lang::Ru => format!("{bits} бит"),
                    });
                }
                match params.gamma.as_deref() {
                    Some("pq") => parts.push("HDR (PQ)".into()),
                    Some("hlg") => parts.push("HDR (HLG)".into()),
                    _ => {}
                }
            }
        }
        Kind::Audio => {
            if let Some(n) = track.channels {
                parts.push(tracks::channels_in(lang, n));
            }
            if let Some(rate) = track.samplerate.filter(|r| *r > 0) {
                let khz = rate as f64 / 1000.0;
                let decimals = if rate % 1000 == 0 { 0 } else { 1 };
                let unit = if lang == Lang::Ru { "кГц" } else { "kHz" };
                parts.push(format!("{} {unit}", decimal_in(lang, khz, decimals)));
            }
        }
        Kind::Sub => {}
    }
    if let Some(bitrate) = track.bitrate.filter(|b| *b > 0) {
        parts.push(bitrate_in(lang, bitrate as f64));
    }
    let flags = [
        (track.external, "external", "внешняя"),
        (track.default, "default", "по умолчанию"),
        (track.forced, "forced", "принудительная"),
    ];
    for (on, en, ru) in flags {
        if on {
            parts.push(if lang == Lang::Ru { ru } else { en }.into());
        }
    }
    parts.join(" · ")
}

/// The bits per component of an FFmpeg pixel format (`yuv420p10` → 10);
/// 8 is not worth saying.
fn bit_depth(pixel_format: &str) -> Option<u32> {
    let digits: String = pixel_format
        .trim_end_matches(|c: char| c.is_ascii_alphabetic())
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let bits = digits.parse().ok()?;
    match pixel_format {
        // p010 and p016 hold 10 and 16 bits.
        "p010" => Some(10),
        "p016" => Some(16),
        _ if pixel_format.starts_with("yuv")
            || pixel_format.starts_with("gbr")
            || pixel_format.starts_with("gray") =>
        {
            (bits > 8 && bits <= 16).then_some(bits)
        }
        _ => None,
    }
}

/// The file's rows: name, folder, size, container, duration, bitrate.
pub fn file_rows_in(lang: Lang, path: Option<&Path>, state: &State) -> Vec<(String, String)> {
    let ru = lang == Lang::Ru;
    let label = |en: &str, ru_text: &str| if ru { ru_text } else { en }.to_owned();
    let mut rows = Vec::new();
    if let Some(path) = path {
        if let Some(name) = path.file_name() {
            rows.push((label("Name", "Имя"), name.to_string_lossy().into_owned()));
        }
        if let Some(dir) = path.parent() {
            rows.push((label("Folder", "Папка"), dir.display().to_string()));
        }
    }
    if let Some(size) = state.file_size {
        rows.push((label("Size", "Размер"), size_in(lang, size)));
    }
    if let Some(format) = &state.file_format {
        rows.push((label("Format", "Формат"), container(format)));
    }
    if let Some(d) = state.duration {
        rows.push((
            label("Duration", "Длительность"),
            crate::format::time(Some(d), false),
        ));
        if let Some(size) = state.file_size.filter(|_| d > 0.0) {
            rows.push((
                label("Bitrate", "Битрейт"),
                bitrate_in(lang, size as f64 * 8.0 / d),
            ));
        }
    }
    rows
}

/// A track's heading: "Audio 2: English — Commentary".
pub fn track_heading_in(lang: Lang, track: &Track, tracks: &[Track]) -> String {
    let n = track.number(tracks);
    let kind = kind_in(lang, track.kind, track.albumart);
    if track.albumart {
        return kind.to_owned();
    }
    format!("{kind} {n}: {}", track.name_in(lang, n))
}

/// Everything the window shows, as text for the clipboard.
pub fn report_in(lang: Lang, path: Option<&Path>, state: &State) -> String {
    let ru = lang == Lang::Ru;
    let mut text = String::new();
    let mut section = |title: &str, rows: Vec<(String, String)>| {
        if rows.is_empty() {
            return;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(title);
        text.push('\n');
        for (label, value) in rows {
            text.push_str(&format!("{label}: {value}\n"));
        }
    };
    section(
        if ru { "Файл" } else { "File" },
        file_rows_in(lang, path, state),
    );
    section(
        if ru { "Теги" } else { "Tags" },
        tags_in(lang, &state.metadata),
    );
    let selected_video = tracks::selected(&state.tracks, Kind::Video).map(|t| t.id);
    let rows = state
        .tracks
        .iter()
        .map(|t| {
            let params = state
                .video_params
                .as_ref()
                .filter(|_| t.kind == Kind::Video && Some(t.id) == selected_video);
            (
                track_heading_in(lang, t, &state.tracks),
                track_details_in(lang, t, params),
            )
        })
        .collect();
    section(if ru { "Дорожки" } else { "Tracks" }, rows);
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn known_tags_first_and_named() {
        let m = tags(&[
            ("ENCODER", "Lavf61"),
            ("ALBUM", "Paranoid"),
            ("TITLE", "War Pigs"),
            ("DATE", "1970-09-18"),
            ("ARTIST", "Black Sabbath"),
            ("REPLAYGAIN_TRACK_GAIN", "-6.1 dB"),
        ]);
        let t = tags_in(Lang::Ru, &m);
        let labels: Vec<&str> = t.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Название",
                "Исполнитель",
                "Альбом",
                "Год",
                "Кодировщик",
                "REPLAYGAIN_TRACK_GAIN"
            ]
        );
        assert_eq!(artist(&m), Some("Black Sabbath"));
        assert_eq!(album(&m).as_deref(), Some("Paranoid (1970)"));
        assert_eq!(tag(&m, &["title"]), Some("War Pigs"));
    }

    #[test]
    fn sizes_and_bitrates() {
        assert_eq!(size_in(Lang::En, 512), "512 bytes");
        assert_eq!(size_in(Lang::Ru, 1_468_006), "1,40 МБ");
        assert_eq!(size_in(Lang::En, 1_471_026_299), "1.37 GB");
        assert_eq!(size_in(Lang::Ru, 160 << 20), "160 МБ");
        assert_eq!(bitrate_in(Lang::En, 320_000.0), "320 kb/s");
        assert_eq!(bitrate_in(Lang::Ru, 8_512_000.0), "8,5 Мбит/с");
        assert_eq!(container("matroska,webm"), "Matroska");
        assert_eq!(container("mov,mp4,m4a,3gp,3g2,mj2"), "MP4 / QuickTime");
        assert_eq!(container("mp3"), "MP3");
    }

    #[test]
    fn bit_depths() {
        assert_eq!(bit_depth("yuv420p10"), Some(10));
        assert_eq!(bit_depth("yuv420p10le"), Some(10));
        assert_eq!(bit_depth("p010"), Some(10));
        assert_eq!(bit_depth("yuv420p"), None);
        assert_eq!(bit_depth("nv12"), None);
    }

    #[test]
    fn track_details() {
        let video = Track {
            id: 1,
            kind: Kind::Video,
            title: None,
            lang: None,
            codec: Some("hevc".into()),
            channels: None,
            layout: None,
            codec_desc: None,
            width: Some(3840),
            height: Some(2160),
            fps: Some(23.976023976),
            samplerate: None,
            bitrate: None,
            albumart: false,
            external: false,
            default: true,
            forced: false,
            selected: true,
        };
        let params = VideoParams {
            pixel_format: Some("yuv420p10".into()),
            primaries: Some("bt.2020".into()),
            gamma: Some("pq".into()),
        };
        assert_eq!(
            track_details_in(Lang::Ru, &video, Some(&params)),
            "HEVC · 3840×2160 · 23,976 fps · 10 бит · HDR (PQ) · по умолчанию"
        );
        let audio = Track {
            kind: Kind::Audio,
            codec: Some("aac".into()),
            channels: Some(6),
            width: None,
            height: None,
            fps: None,
            samplerate: Some(48000),
            bitrate: Some(384_000),
            default: false,
            ..video
        };
        assert_eq!(
            track_details_in(Lang::En, &audio, None),
            "AAC · 5.1 · 48 kHz · 384 kb/s"
        );
    }
}
