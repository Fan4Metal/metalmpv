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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Kind {
    #[default]
    Video,
    Audio,
    Sub,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    /// mpv's id (`aid`/`sid`), counted per kind from 1.
    pub id: i64,
    /// The track's id in the file (Matroska's track number), when mpv
    /// knows it (`src-id`).
    pub src_id: Option<i64>,
    /// FFmpeg's stream index (`ff-index`).
    pub ff_index: Option<i64>,
    pub kind: Kind,
    pub title: Option<String>,
    /// ISO 639 code as in the file (`rus`, `en`).
    pub lang: Option<String>,
    /// FFmpeg's codec name (`ac3`, `subrip`).
    pub codec: Option<String>,
    pub channels: Option<i64>,
    /// The channel layout (`5.1(side)`).
    pub layout: Option<String>,
    /// FFmpeg's long codec name ("H.265 / HEVC (High Efficiency Video Coding)").
    pub codec_desc: Option<String>,
    /// The codec's profile ("High", "Main 10", "LC", "DTS-HD MA").
    pub profile: Option<String>,
    /// A picture's size in pixels as the file states it.
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// The pixels' aspect ratio, 1 for square pixels.
    pub par: Option<f64>,
    /// Degrees clockwise the picture is turned for display.
    pub rotation: Option<i64>,
    /// Dolby Vision's profile and level.
    pub dolby_vision: Option<(i64, i64)>,
    pub fps: Option<f64>,
    /// Audio samples per second.
    pub samplerate: Option<i64>,
    /// Bits per second as the file states it.
    pub bitrate: Option<i64>,
    /// A cover picture, not a video.
    pub albumart: bool,
    /// Loaded from a file of its own (subtitles next to the video).
    pub external: bool,
    pub default: bool,
    pub forced: bool,
    pub commentary: bool,
    pub original: bool,
    pub hearing_impaired: bool,
    pub visual_impaired: bool,
    pub selected: bool,
    /// The track's own tags, as the file names them; the language and
    /// FFmpeg's default handler names left out.
    pub tags: Vec<(String, String)>,
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
                src_id: t.get("src-id").and_then(Value::int),
                ff_index: t.get("ff-index").and_then(Value::int),
                kind,
                title: text("title"),
                lang: text("lang"),
                codec: text("codec"),
                channels: t.get("demux-channel-count").and_then(Value::int),
                layout: text("demux-channels"),
                codec_desc: text("codec-desc"),
                profile: text("codec-profile"),
                width: t.get("demux-w").and_then(Value::int),
                height: t.get("demux-h").and_then(Value::int),
                par: t
                    .get("demux-par")
                    .and_then(Value::double)
                    .filter(|p| *p > 0.0),
                rotation: t
                    .get("demux-rotation")
                    .and_then(Value::int)
                    .filter(|r| *r != 0),
                dolby_vision: t.get("dolby-vision-profile").and_then(Value::int).map(|p| {
                    (
                        p,
                        t.get("dolby-vision-level")
                            .and_then(Value::int)
                            .unwrap_or(0),
                    )
                }),
                fps: t.get("demux-fps").and_then(Value::double),
                samplerate: t.get("demux-samplerate").and_then(Value::int),
                bitrate: t.get("demux-bitrate").and_then(Value::int),
                albumart: flag("albumart"),
                external: flag("external"),
                default: flag("default"),
                forced: flag("forced"),
                commentary: flag("commentary"),
                original: flag("original"),
                hearing_impaired: flag("hearing-impaired"),
                visual_impaired: flag("visual-impaired"),
                selected: flag("selected"),
                tags: t.get("metadata").map(track_tags).unwrap_or_default(),
            })
        })
        .collect()
}

/// A track's `metadata` map without what says nothing new: the language
/// and the title (the track has them), FFmpeg's default handler names
/// ("SoundHandler"), an MP4's empty vendor ("[0][0][0][0]") and empty
/// values.
fn track_tags(metadata: &Value) -> Vec<(String, String)> {
    let Value::Map(entries) = metadata else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|(k, v)| Some((k.clone(), v.string()?.trim().to_owned())))
        .filter(|(k, v)| {
            !v.is_empty()
                && !k.eq_ignore_ascii_case("language")
                && !k.eq_ignore_ascii_case("title")
                && !(k.eq_ignore_ascii_case("handler_name") && v.ends_with("Handler"))
                && !(k.eq_ignore_ascii_case("vendor_id") && v.chars().all(|c| "[0]".contains(c)))
        })
        .collect()
}

/// What a track chosen by hand is like: its language and title, to choose
/// the most alike in a file opened for the first time (the same voice-over
/// in the next episode of a series; `prefer_last_tracks`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Likeness {
    /// The language as English names it, in lower case ("russian").
    pub lang: Option<String>,
    /// The title, in lower case.
    pub title: Option<String>,
}

impl Likeness {
    pub fn of(track: &Track) -> Self {
        Self {
            lang: track
                .lang
                .as_deref()
                .map(|code| language_in(Lang::En, code).to_lowercase()),
            title: track
                .title
                .as_deref()
                .map(|t| t.trim().to_lowercase())
                .filter(|t| !t.is_empty()),
        }
    }

    /// How alike `track` is: the title counts more than the language.
    fn score(&self, track: &Track) -> u8 {
        let other = Self::of(track);
        let same = |a: &Option<String>, b: &Option<String>| a.is_some() && a == b;
        2 * u8::from(same(&self.title, &other.title)) + u8::from(same(&self.lang, &other.lang))
    }

    /// `language<TAB>title`, empty for what is not known.
    pub fn to_text(&self) -> String {
        format!(
            "{}\t{}",
            self.lang.as_deref().unwrap_or_default(),
            self.title.as_deref().unwrap_or_default()
        )
    }

    pub fn parse(text: &str) -> Option<Self> {
        let (lang, title) = text.split_once('\t')?;
        let known = |s: &str| (!s.is_empty()).then(|| s.to_owned());
        let like = Self {
            lang: known(lang),
            title: known(title),
        };
        (like != Self::default()).then_some(like)
    }
}

/// The track of `kind` most like `like`, the first of equals; `None` when
/// none shares its title or language.
pub fn most_like(tracks: &[Track], kind: Kind, like: &Likeness) -> Option<i64> {
    let mut best: Option<(u8, i64)> = None;
    for t in of_kind(tracks, kind) {
        let score = like.score(t);
        if score > 0 && best.is_none_or(|(s, _)| score > s) {
            best = Some((score, t.id));
        }
    }
    best.map(|(_, id)| id)
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
    /// Its channels as people name them: the layout's number when mpv
    /// gives one ("5.1" of "5.1(side)", "3.0"), else by their count.
    pub fn channels_label(&self, lang: Lang) -> Option<String> {
        let n = self.channels?;
        let named = self.layout.as_deref().and_then(|layout| {
            let base = layout.split('(').next()?;
            (base.contains('.') && base.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
                .then(|| base.to_owned())
        });
        Some(named.unwrap_or_else(|| channels_in(lang, n)))
    }

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
        let title = words(title);
        let names = [
            language_in(Lang::En, code),
            language_in(Lang::Ru, code),
            code.clone(),
        ];
        // Whole words: "en" is in "Commentary", "no" in "Mono".
        names.iter().any(|n| {
            let name = words(n);
            !name.is_empty() && title.windows(name.len()).any(|w| w == name.as_slice())
        })
    }

    /// The format as people name it: the codec, made precise by the
    /// profile FFmpeg gives a sound track ("DTS-HD MA", "DTS:X",
    /// "E-AC3 Atmos", "HE-AAC").
    pub fn format_name(&self) -> Option<String> {
        let codec = self.codec.as_deref()?;
        let name = codec_name(codec);
        let Some(profile) = self.profile.as_deref().filter(|_| self.kind == Kind::Audio) else {
            return Some(name);
        };
        Some(match codec {
            // "DTS-HD MA + DTS:X IMAX" says the most at its end.
            "dts" if profile.contains("DTS:X") => {
                profile.rsplit(" + ").next().unwrap_or(profile).to_owned()
            }
            "dts" if profile.starts_with("DTS") => profile.to_owned(),
            "eac3" | "truehd" | "ac3" if profile.contains("Atmos") => format!("{name} Atmos"),
            "aac" if profile.starts_with("HE-AAC") => profile.to_owned(),
            _ => name,
        })
    }

    /// The format, the channels and the marks (external, forced, default),
    /// each shown as a badge under the name.
    pub fn badges_in(&self, lang: Lang) -> Vec<String> {
        let ru = lang == Lang::Ru;
        let mut badges = Vec::new();
        if let Some(format) = self.format_name() {
            badges.push(format);
        }
        if let Some(channels) = self.channels_label(lang) {
            badges.push(channels);
        }
        // Subtitles are plural in Russian, a track of sound is not.
        let one = self.kind != Kind::Sub;
        for (on, en, ru_text) in [
            (
                self.external,
                "external",
                if one {
                    "внешняя"
                } else {
                    "внешние"
                },
            ),
            (
                self.forced,
                "forced",
                if one {
                    "принудительная"
                } else {
                    "принудительные"
                },
            ),
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
pub fn codec_name(codec: &str) -> String {
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
        "dtshd" => "DTS-HD".into(),
        "vorbis" => "Vorbis".into(),
        "opus" => "Opus".into(),
        "h264" => "H.264".into(),
        "mpeg2video" => "MPEG-2".into(),
        "mpeg4" => "MPEG-4".into(),
        "mjpeg" => "JPEG".into(),
        c if c.starts_with("pcm_") => "PCM".into(),
        c => c.to_uppercase(),
    }
}

/// The places after the point a sample rate in kHz needs: 48, 44.1,
/// 22.05, 11.025.
pub fn khz_decimals(rate: i64) -> usize {
    match rate {
        r if r % 1000 == 0 => 0,
        r if r % 100 == 0 => 1,
        r if r % 10 == 0 => 2,
        _ => 3,
    }
}

pub fn channels_in(lang: Lang, n: i64) -> String {
    match (n, lang) {
        (1, Lang::En) => "mono".into(),
        (1, Lang::Ru) => "моно".into(),
        (2, _) => "2.0".into(),
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

/// Which subtitles a file opened for the first time shows, unless ones
/// like those last chosen by hand are found (`prefer_last_tracks`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SubMode {
    /// As the file marks them (a default or a forced track).
    #[default]
    File,
    Off,
    /// Only those marked forced: signs and foreign lines.
    Forced,
    /// In the languages chosen, in their order, else none (forced ones in
    /// the sound's language still show).
    Languages,
}

impl SubMode {
    pub const ALL: [SubMode; 4] = [
        SubMode::File,
        SubMode::Off,
        SubMode::Forced,
        SubMode::Languages,
    ];

    /// The stored name.
    pub fn name(self) -> &'static str {
        match self {
            SubMode::File => "file",
            SubMode::Off => "off",
            SubMode::Forced => "forced",
            SubMode::Languages => "languages",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == name)
    }

    pub fn label(self) -> &'static str {
        match self {
            SubMode::File => tr!("As the file marks them", "Как отмечено в файле"),
            SubMode::Off => tr!("Off", "Выключены"),
            SubMode::Forced => tr!("Forced only", "Только принудительные"),
            SubMode::Languages => tr!("In the languages below", "На языках ниже"),
        }
    }
}

/// Which subtitle files beside a video are loaded with it (mpv's
/// `sub-auto`), there and in its `Subs`, `Subtitles` or `Sub` folder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SubAuto {
    /// The video's name exactly: `Film.srt`.
    Exact,
    /// Names with the video's in them: `Film.rus.srt`, `Film.Forced.ass`.
    #[default]
    Fuzzy,
    /// Every subtitle file of the folder.
    All,
    Off,
}

impl SubAuto {
    pub const ALL: [SubAuto; 4] = [SubAuto::Exact, SubAuto::Fuzzy, SubAuto::All, SubAuto::Off];

    /// The stored name, mpv's too (`no` for off).
    pub fn name(self) -> &'static str {
        match self {
            SubAuto::Exact => "exact",
            SubAuto::Fuzzy => "fuzzy",
            SubAuto::All => "all",
            SubAuto::Off => "off",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == name)
    }

    pub fn label(self) -> &'static str {
        match self {
            SubAuto::Exact => tr!("The same name", "С тем же именем"),
            SubAuto::Fuzzy => tr!("Names with the video's", "С именем видео в названии"),
            SubAuto::All => tr!("All in the folder", "Все в папке"),
            SubAuto::Off => tr!("None", "Никакие"),
        }
    }

    fn option(self) -> &'static str {
        match self {
            SubAuto::Off => "no",
            other => other.name(),
        }
    }
}

/// The tracks a file opened for the first time starts with: the sound's
/// languages and the subtitles' (in order), which subtitles show, and the
/// subtitle files loaded with it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrackPrefs {
    /// Language codes as kept (`rus`), at most [`TrackPrefs::MAX`].
    pub audio: Vec<String>,
    pub sub_mode: SubMode,
    pub subs: Vec<String>,
    pub sub_auto: SubAuto,
}

impl TrackPrefs {
    /// Languages kept for the sound and for the subtitles each.
    pub const MAX: usize = 3;

    /// mpv's options for them. A file's language may be written with two
    /// letters or three: each language is given with all its codes.
    pub fn options(&self) -> Vec<(&'static str, String)> {
        let codes = |list: &[String]| -> String {
            list.iter()
                .flat_map(|code| codes_of(code))
                .collect::<Vec<_>>()
                .join(",")
        };
        let slang = match self.sub_mode {
            SubMode::Languages => codes(&self.subs),
            _ => String::new(),
        };
        // Without a language matched: the track the file marks default
        // (`subs-fallback`), and a forced one (`subs-fallback-forced`).
        let (fallback, forced) = match self.sub_mode {
            SubMode::File => ("default", "yes"),
            SubMode::Off => ("no", "no"),
            SubMode::Forced => ("no", "always"),
            SubMode::Languages => ("no", "yes"),
        };
        vec![
            ("alang", codes(&self.audio)),
            ("slang", slang),
            ("subs-fallback", fallback.into()),
            ("subs-fallback-forced", forced.into()),
            ("sub-auto", self.sub_auto.option().into()),
            ("sub-file-paths", "Subs;Subtitles;Sub".into()),
        ]
    }

    /// `rus,eng` as kept: known codes only, each once, at most [`Self::MAX`].
    pub fn languages_from(text: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for code in text.split(',').map(str::trim) {
            if let Some(code) = language_code(code)
                && !out.iter().any(|c| c == code)
                && out.len() < Self::MAX
            {
                out.push(code.to_owned());
            }
        }
        out
    }
}

/// The language codes as kept, with their names in `lang`, for a list to
/// choose from.
pub fn languages(lang: Lang) -> Vec<(&'static str, &'static str)> {
    LANGUAGES
        .iter()
        .map(|(codes, en, ru)| {
            let name = match lang {
                Lang::En => *en,
                Lang::Ru => *ru,
            };
            (kept_code(codes), name)
        })
        .collect()
}

/// The code a language is kept under: its first of three letters.
fn kept_code(codes: &[&'static str]) -> &'static str {
    codes.iter().find(|c| c.len() == 3).unwrap_or(&codes[0])
}

/// The kept code of language `code` (any of its codes), if known.
fn language_code(code: &str) -> Option<&'static str> {
    let lower = code.to_ascii_lowercase();
    LANGUAGES
        .iter()
        .find(|(codes, _, _)| codes.contains(&lower.as_str()))
        .map(|(codes, _, _)| kept_code(codes))
}

/// All the codes of language `code` (`rus` gives `rus` and `ru`).
fn codes_of(code: &str) -> Vec<&str> {
    let lower = code.to_ascii_lowercase();
    match LANGUAGES
        .iter()
        .find(|(codes, _, _)| codes.contains(&lower.as_str()))
    {
        Some((codes, _, _)) => {
            let mut all = vec![kept_code(codes)];
            all.extend(codes.iter().filter(|c| **c != kept_code(codes)));
            all
        }
        None => vec![code],
    }
}

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

/// The words of `text` in lower case.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_preferences_as_mpv_options() {
        let prefs = TrackPrefs {
            audio: TrackPrefs::languages_from("rus, en,xx,rus,ger,fre"),
            sub_mode: SubMode::Languages,
            subs: vec!["eng".into()],
            sub_auto: SubAuto::Off,
        };
        assert_eq!(prefs.audio, ["rus", "eng", "ger"]);
        let options = prefs.options();
        let get = |name| options.iter().find(|(n, _)| *n == name).unwrap().1.clone();
        assert_eq!(get("alang"), "rus,ru,eng,en,ger,de,deu");
        assert_eq!(get("slang"), "eng,en");
        assert_eq!(get("subs-fallback"), "no");
        assert_eq!(get("sub-auto"), "no");
        let off = TrackPrefs {
            sub_mode: SubMode::Forced,
            ..TrackPrefs::default()
        };
        let options = off.options();
        let get = |name| options.iter().find(|(n, _)| *n == name).unwrap().1.clone();
        assert_eq!(get("slang"), "");
        assert_eq!(get("subs-fallback-forced"), "always");
        assert_eq!(get("sub-auto"), "fuzzy");
        assert!(languages(Lang::Ru).contains(&("rus", "Русский")));
    }

    #[test]
    fn format_names() {
        let audio = |codec: &str, profile: Option<&str>| Track {
            kind: Kind::Audio,
            codec: Some(codec.into()),
            profile: profile.map(Into::into),
            ..Track::default()
        };
        let name = |t: Track| t.format_name().unwrap();
        assert_eq!(name(audio("dts", Some("DTS-HD MA"))), "DTS-HD MA");
        assert_eq!(name(audio("dts", Some("DTS-HD MA + DTS:X"))), "DTS:X");
        assert_eq!(
            name(audio("dts", Some("DTS-HD MA + DTS:X IMAX"))),
            "DTS:X IMAX"
        );
        assert_eq!(
            name(audio("eac3", Some("Dolby Digital Plus + Dolby Atmos"))),
            "E-AC3 Atmos"
        );
        assert_eq!(
            name(audio("truehd", Some("Dolby TrueHD + Dolby Atmos"))),
            "TrueHD Atmos"
        );
        assert_eq!(name(audio("aac", Some("HE-AACv2"))), "HE-AACv2");
        assert_eq!(name(audio("aac", Some("LC"))), "AAC");
        assert_eq!(name(audio("dts", None)), "DTS");
        let video = Track {
            codec: Some("hevc".into()),
            profile: Some("Main 10".into()),
            ..Track::default()
        };
        assert_eq!(name(video), "HEVC");
    }

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
    fn channels_rates_and_flags_as_people_say_them() {
        let audio = |extra| from_value(&Value::List(vec![track(1, "audio", extra)])).remove(0);
        let three = audio(vec![
            ("demux-channel-count", Value::Int(3)),
            ("demux-channels", Value::Str("3.0".into())),
        ]);
        assert_eq!(three.channels_label(Lang::En).as_deref(), Some("3.0"));
        let side = audio(vec![
            ("demux-channel-count", Value::Int(6)),
            ("demux-channels", Value::Str("5.1(side)".into())),
        ]);
        assert_eq!(side.channels_label(Lang::En).as_deref(), Some("5.1"));
        let bare = audio(vec![("demux-channel-count", Value::Int(3))]);
        assert_eq!(bare.channels_label(Lang::En).as_deref(), Some("3 ch"));
        assert_eq!([48000, 44100, 22050, 11025].map(khz_decimals), [0, 1, 2, 3]);
        let forced = audio(vec![("forced", Value::Flag(true))]);
        assert!(
            forced
                .badges_in(Lang::Ru)
                .contains(&"принудительная".to_owned())
        );
        let t = sample();
        assert!(t[3].badges_in(Lang::Ru).contains(&"внешние".to_owned()));
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
        // The code is a word of its own, not a part of one.
        t[2].title = Some("Commentary".into());
        t[2].lang = Some("en".into());
        assert_eq!(t[2].name_in(Lang::En, 2), "English — Commentary");
        t[2].title = Some("Mono".into());
        t[2].lang = Some("no".into());
        assert!(t[2].name_in(Lang::En, 2).ends_with("— Mono"));
        t[2].title = Some("rus, 2.0".into());
        t[2].lang = Some("rus".into());
        assert_eq!(t[2].name_in(Lang::En, 2), "rus, 2.0");
        assert_eq!(language_in(Lang::En, "pt-BR"), "Portuguese");
        assert_eq!(language_in(Lang::Ru, "xyz"), "XYZ");
    }

    #[test]
    fn the_most_alike_track() {
        let episode = |tracks: &[(&str, Option<&str>)]| -> Vec<Track> {
            tracks
                .iter()
                .enumerate()
                .map(|(i, (lang, title))| Track {
                    id: i as i64 + 1,
                    kind: Kind::Audio,
                    lang: Some((*lang).into()),
                    title: title.map(str::to_owned),
                    ..Track::default()
                })
                .collect()
        };
        let first = episode(&[
            ("eng", None),
            ("rus", Some("Дубляж")),
            ("rus", Some("Многоголосый")),
        ]);
        let like = Likeness::of(&first[2]);
        assert_eq!(Likeness::parse(&like.to_text()), Some(like.clone()));
        // The same voice-over in another order, its title in another case.
        let next = episode(&[
            ("rus", Some("Дубляж")),
            ("eng", None),
            ("ru", Some("МНОГОГОЛОСЫЙ")),
        ]);
        assert_eq!(most_like(&next, Kind::Audio, &like), Some(3));
        // No such title: the first of the same language.
        let other = episode(&[("eng", None), ("rus", Some("Дубляж"))]);
        assert_eq!(most_like(&other, Kind::Audio, &like), Some(2));
        // Nothing alike: mpv's choice stays.
        let none = episode(&[("eng", None), ("jpn", None)]);
        assert_eq!(most_like(&none, Kind::Audio, &like), None);
        assert_eq!(Likeness::parse("\t"), None);
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
