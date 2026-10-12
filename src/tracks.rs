//! The audio and subtitle tracks of the playing file, from mpv's
//! `track-list`, with their names as the interface shows them and the
//! order the keys cycle through them in.

use crate::i18n::Lang;
use crate::mpv::Value;

/// Extensions of subtitle files: the file dialog's filter, and dropped
/// files that go with the playing video instead of replacing it.
pub const SUBTITLE_EXTENSIONS: &[&str] = &[
    "srt", "ass", "ssa", "sub", "idx", "vtt", "sup", "smi", "txt",
];

/// Whether `path` is a subtitle file by its extension (`.txt` aside: too
/// many text files are not subtitles).
pub fn is_subtitles(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        !e.eq_ignore_ascii_case("txt")
            && SUBTITLE_EXTENSIONS
                .iter()
                .any(|s| s.eq_ignore_ascii_case(e))
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Video,
    Audio,
    Sub,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    /// mpv's id (`aid`/`sid`), counted per kind from 1.
    pub id: i64,
    pub kind: Kind,
    pub title: Option<String>,
    /// ISO 639 code as in the file (`rus`, `en`).
    pub lang: Option<String>,
    /// FFmpeg's codec name (`ac3`, `subrip`).
    pub codec: Option<String>,
    pub channels: Option<i64>,
    /// Loaded from a file of its own (subtitles next to the video).
    pub external: bool,
    pub default: bool,
    pub forced: bool,
    pub selected: bool,
}

/// The tracks of a `track-list` value; entries that are not understood
/// are left out.
pub fn from_value(list: &Value) -> Vec<Track> {
    list.list()
        .iter()
        .filter_map(|t| {
            let kind = match t.get("type")?.string()? {
                "video" => Kind::Video,
                "audio" => Kind::Audio,
                "sub" => Kind::Sub,
                _ => return None,
            };
            let text = |key| {
                t.get(key)
                    .and_then(Value::string)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
            };
            let flag = |key| t.get(key).is_some_and(Value::flag);
            Some(Track {
                id: t.get("id")?.int()?,
                kind,
                title: text("title"),
                lang: text("lang"),
                codec: text("codec"),
                channels: t.get("demux-channel-count").and_then(Value::int),
                external: flag("external"),
                default: flag("default"),
                forced: flag("forced"),
                selected: flag("selected"),
            })
        })
        .collect()
}

/// The tracks of `kind`, in the file's order.
pub fn of_kind(tracks: &[Track], kind: Kind) -> Vec<&Track> {
    tracks.iter().filter(|t| t.kind == kind).collect()
}

/// The selected track of `kind`.
pub fn selected(tracks: &[Track], kind: Kind) -> Option<&Track> {
    tracks.iter().find(|t| t.kind == kind && t.selected)
}

/// The track after the selected one of `kind`, round: audio goes from the
/// last back to the first; subtitles pass through "off" (`None`) after the
/// last. `None` overall when there is nothing to switch to.
pub fn next(tracks: &[Track], kind: Kind) -> Option<Option<i64>> {
    let ids: Vec<i64> = of_kind(tracks, kind).iter().map(|t| t.id).collect();
    if ids.is_empty() {
        return None;
    }
    let current = selected(tracks, kind).map(|t| t.id);
    let position = current.and_then(|c| ids.iter().position(|id| *id == c));
    match (kind, position) {
        (Kind::Sub, None) => Some(Some(ids[0])),
        (Kind::Sub, Some(i)) if i + 1 == ids.len() => Some(None),
        (_, Some(i)) => Some(Some(ids[(i + 1) % ids.len()])),
        (_, None) => Some(Some(ids[0])),
    }
    .filter(|next| ids.len() > 1 || kind == Kind::Sub || *next != current)
}

impl Track {
    /// The track's number among those of its kind, from 1.
    pub fn number(&self, tracks: &[Track]) -> usize {
        of_kind(tracks, self.kind)
            .iter()
            .position(|t| t.id == self.id)
            .map_or(0, |i| i + 1)
    }

    /// What a track is chosen by: the language, then the title after a
    /// dash ("Русский — Superbit"); the title alone when it names the
    /// language itself or there is none; "Track N" when neither is known.
    pub fn name_in(&self, lang: Lang, number: usize) -> String {
        let language = self.lang.as_deref().map(|c| language_in(lang, c));
        match (language, &self.title) {
            (Some(l), Some(t)) if !self.title_names_language() => format!("{l} — {t}"),
            (_, Some(t)) => t.clone(),
            (Some(l), None) => l,
            (None, None) => match lang {
                Lang::En => format!("Track {number}"),
                Lang::Ru => format!("Дорожка {number}"),
            },
        }
    }

    /// Whether the title already says the language ("Russian Dub",
    /// "Русский", "rus").
    fn title_names_language(&self) -> bool {
        let (Some(title), Some(code)) = (&self.title, &self.lang) else {
            return false;
        };
        let title = title.to_lowercase();
        let names = [
            language_in(Lang::En, code),
            language_in(Lang::Ru, code),
            code.clone(),
        ];
        names.iter().any(|n| title.contains(&n.to_lowercase()))
    }

    /// The format, the channels and the marks (external, forced, default),
    /// each shown as a badge under the name.
    pub fn badges_in(&self, lang: Lang) -> Vec<String> {
        let ru = lang == Lang::Ru;
        let mut badges = Vec::new();
        if let Some(codec) = &self.codec {
            badges.push(codec_name(codec));
        }
        if let Some(n) = self.channels {
            badges.push(channels_in(lang, n));
        }
        for (on, en, ru_text) in [
            (self.external, "external", "внешние"),
            (self.forced, "forced", "принудительные"),
            (self.default, "default", "по умолчанию"),
        ] {
            if on {
                badges.push(if ru { ru_text } else { en }.into());
            }
        }
        badges
    }

    /// The badges in one line: "AC3 · 5.1 · default".
    pub fn details_in(&self, lang: Lang) -> String {
        self.badges_in(lang).join(" · ")
    }

    /// "2/3: English (AAC · 2.0)", for the OSD.
    pub fn summary_in(&self, lang: Lang, tracks: &[Track]) -> String {
        let n = self.number(tracks);
        let total = of_kind(tracks, self.kind).len();
        let details = self.details_in(lang);
        let name = self.name_in(lang, n);
        if details.is_empty() {
            format!("{n}/{total}: {name}")
        } else {
            format!("{n}/{total}: {name} ({details})")
        }
    }
}

/// The codec as people know the format.
fn codec_name(codec: &str) -> String {
    match codec {
        "subrip" | "srt" => "SRT".into(),
        "ass" => "ASS".into(),
        "ssa" => "SSA".into(),
        "hdmv_pgs_subtitle" => "PGS".into(),
        "dvd_subtitle" => "VobSub".into(),
        "dvb_subtitle" => "DVB".into(),
        "webvtt" => "WebVTT".into(),
        "mov_text" => "TX3G".into(),
        "eac3" => "E-AC3".into(),
        "truehd" => "TrueHD".into(),
        "vorbis" => "Vorbis".into(),
        "opus" => "Opus".into(),
        c if c.starts_with("pcm_") => "PCM".into(),
        c => c.to_uppercase(),
    }
}

fn channels_in(lang: Lang, n: i64) -> String {
    match (n, lang) {
        (1, Lang::En) => "mono".into(),
        (1, Lang::Ru) => "моно".into(),
        (2, _) => "2.0".into(),
        (3, _) => "2.1".into(),
        (6, _) => "5.1".into(),
        (8, _) => "7.1".into(),
        (n, Lang::En) => format!("{n} ch"),
        (n, Lang::Ru) => format!("{n} кан."),
    }
}

/// Languages by their ISO 639-1 and 639-2 codes, in English and Russian.
const LANGUAGES: &[(&[&str], &str, &str)] = &[
    (&["ru", "rus"], "Russian", "Русский"),
    (&["en", "eng"], "English", "Английский"),
    (&["uk", "ukr"], "Ukrainian", "Украинский"),
    (&["be", "bel"], "Belarusian", "Белорусский"),
    (&["kk", "kaz"], "Kazakh", "Казахский"),
    (&["de", "ger", "deu"], "German", "Немецкий"),
    (&["fr", "fre", "fra"], "French", "Французский"),
    (&["es", "spa"], "Spanish", "Испанский"),
    (&["it", "ita"], "Italian", "Итальянский"),
    (&["pt", "por"], "Portuguese", "Португальский"),
    (&["pl", "pol"], "Polish", "Польский"),
    (&["cs", "cze", "ces"], "Czech", "Чешский"),
    (&["nl", "dut", "nld"], "Dutch", "Нидерландский"),
    (&["sv", "swe"], "Swedish", "Шведский"),
    (&["fi", "fin"], "Finnish", "Финский"),
    (&["no", "nor", "nob"], "Norwegian", "Норвежский"),
    (&["da", "dan"], "Danish", "Датский"),
    (&["hu", "hun"], "Hungarian", "Венгерский"),
    (&["ro", "rum", "ron"], "Romanian", "Румынский"),
    (&["bg", "bul"], "Bulgarian", "Болгарский"),
    (&["el", "gre", "ell"], "Greek", "Греческий"),
    (&["tr", "tur"], "Turkish", "Турецкий"),
    (&["ar", "ara"], "Arabic", "Арабский"),
    (&["he", "heb"], "Hebrew", "Иврит"),
    (&["hi", "hin"], "Hindi", "Хинди"),
    (&["ja", "jpn"], "Japanese", "Японский"),
    (&["zh", "chi", "zho"], "Chinese", "Китайский"),
    (&["ko", "kor"], "Korean", "Корейский"),
];

/// The language of `code` in the interface's language; an unknown code
/// as it is, in capitals.
pub fn language_in(lang: Lang, code: &str) -> String {
    let lower = code.to_ascii_lowercase();
    let base = lower.split(['-', '_']).next().unwrap_or_default();
    match LANGUAGES.iter().find(|(codes, _, _)| codes.contains(&base)) {
        Some((_, en, ru)) => match lang {
            Lang::En => (*en).into(),
            Lang::Ru => (*ru).into(),
        },
        None => code.to_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i64, kind: &str, extra: Vec<(&str, Value)>) -> Value {
        let mut entries = vec![
            ("id".to_owned(), Value::Int(id)),
            ("type".to_owned(), Value::Str(kind.into())),
        ];
        entries.extend(extra.into_iter().map(|(k, v)| (k.to_owned(), v)));
        Value::Map(entries)
    }

    fn sample() -> Vec<Track> {
        from_value(&Value::List(vec![
            track(
                1,
                "video",
                vec![
                    ("codec", Value::Str("hevc".into())),
                    ("selected", Value::Flag(true)),
                ],
            ),
            track(
                1,
                "audio",
                vec![
                    ("lang", Value::Str("rus".into())),
                    ("codec", Value::Str("ac3".into())),
                    ("demux-channel-count", Value::Int(6)),
                    ("default", Value::Flag(true)),
                    ("selected", Value::Flag(true)),
                ],
            ),
            track(
                2,
                "audio",
                vec![
                    ("lang", Value::Str("eng".into())),
                    ("title", Value::Str("Commentary".into())),
                    ("codec", Value::Str("aac".into())),
                    ("demux-channel-count", Value::Int(2)),
                ],
            ),
            track(
                1,
                "sub",
                vec![
                    ("lang", Value::Str("ru".into())),
                    ("codec", Value::Str("subrip".into())),
                    ("external", Value::Flag(true)),
                ],
            ),
            track(
                2,
                "sub",
                vec![("codec", Value::Str("hdmv_pgs_subtitle".into()))],
            ),
            Value::Str("not a track".into()),
        ]))
    }

    #[test]
    fn reads_the_track_list() {
        let t = sample();
        assert_eq!(t.len(), 5);
        assert_eq!(of_kind(&t, Kind::Audio).len(), 2);
        assert_eq!(selected(&t, Kind::Audio).map(|t| t.id), Some(1));
        assert_eq!(selected(&t, Kind::Sub), None);
        assert_eq!(t[1].channels, Some(6));
        assert!(t[3].external);
    }

    #[test]
    fn names_and_details() {
        let mut t = sample();
        assert_eq!(t[1].name_in(Lang::Ru, 1), "Русский");
        assert_eq!(t[1].details_in(Lang::Ru), "AC3 · 5.1 · по умолчанию");
        assert_eq!(t[2].name_in(Lang::En, 2), "English — Commentary");
        assert_eq!(t[2].name_in(Lang::Ru, 2), "Английский — Commentary");
        assert_eq!(t[2].details_in(Lang::En), "AAC · 2.0");
        assert_eq!(t[3].details_in(Lang::En), "SRT · external");
        assert_eq!(t[4].name_in(Lang::Ru, 2), "Дорожка 2");
        assert_eq!(t[4].details_in(Lang::Ru), "PGS");
        assert_eq!(
            t[2].summary_in(Lang::En, &t),
            "2/2: English — Commentary (AAC · 2.0)"
        );
        // A title that says the language is the name alone.
        t[2].title = Some("English (original)".into());
        assert_eq!(t[2].name_in(Lang::Ru, 2), "English (original)");
        t[2].title = Some("Русский дубляж".into());
        t[2].lang = Some("rus".into());
        assert_eq!(t[2].name_in(Lang::En, 2), "Русский дубляж");
        assert_eq!(language_in(Lang::En, "pt-BR"), "Portuguese");
        assert_eq!(language_in(Lang::Ru, "xyz"), "XYZ");
    }

    #[test]
    fn cycling() {
        let mut t = sample();
        assert_eq!(next(&t, Kind::Audio), Some(Some(2)));
        assert_eq!(next(&t, Kind::Sub), Some(Some(1)));
        t[4].selected = true;
        assert_eq!(next(&t, Kind::Sub), Some(None));
        t[4].selected = false;
        t[3].selected = true;
        assert_eq!(next(&t, Kind::Sub), Some(Some(2)));
        // One audio track: nothing to switch to.
        let one: Vec<Track> = t
            .iter()
            .filter(|x| !(x.kind == Kind::Audio && x.id == 2))
            .cloned()
            .collect();
        assert_eq!(next(&one, Kind::Audio), None);
        assert_eq!(next(&[], Kind::Sub), None);
    }
}
