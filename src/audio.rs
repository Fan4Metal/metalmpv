//! The sound's settings and what mpv makes of them: the output device
//! (`audio-device`, from `audio-device-list`), the channels
//! (`audio-channels`), the volume boost (`volume-max`), the normalization
//! (an `af` filter) and the delay against the picture (`audio-delay`).

use crate::mpv::Value;

/// The step of the audio delay keys, in seconds.
pub const DELAY_STEP: f64 = 0.1;
/// The volume's ceilings to choose from, in percent: 100 is no boost.
pub const BOOSTS: [f64; 4] = [100.0, 130.0, 150.0, 200.0];
/// The highest volume there may be, in percent.
pub const MAX_VOLUME: f64 = 200.0;
/// How strongly the normalization evens the volume out: FFmpeg's dynamic
/// normalizer, which raises quiet passages and lowers loud ones as the
/// film plays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Normalize {
    /// Raised 3 times at most, slowly: the change is hardly heard.
    Soft,
    /// Raised 5 times at most (the only one before).
    #[default]
    Medium,
    /// Raised 10 times at most, quickly: whispers and blasts alike, at night.
    Strong,
}

impl Normalize {
    pub const ALL: [Normalize; 3] = [Normalize::Soft, Normalize::Medium, Normalize::Strong];

    /// The stored name.
    pub fn name(self) -> &'static str {
        match self {
            Normalize::Soft => "soft",
            Normalize::Medium => "medium",
            Normalize::Strong => "strong",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|n| n.name() == name)
    }

    pub fn label(self) -> &'static str {
        match self {
            Normalize::Soft => tr!("Soft", "Мягкая"),
            Normalize::Medium => tr!("Medium", "Средняя"),
            Normalize::Strong => tr!("Strong", "Сильная"),
        }
    }

    /// mpv's `af`: `f` the frame in ms (how quickly it follows), `g` the
    /// frames it smooths over (odd), `m` the most it raises. It sounds
    /// only once it holds f × g of sound: kept to Medium's 7.75 s at most
    /// (Soft at 500 × 31 waited 15 s after every seek).
    pub fn filter(self) -> &'static str {
        match self {
            Normalize::Soft => "lavfi=[dynaudnorm=f=400:g=19:p=0.5:m=3:r=0.9:b=1]",
            Normalize::Medium => "lavfi=[dynaudnorm=f=250:g=31:p=0.5:m=5:r=0.9:b=1]",
            Normalize::Strong => "lavfi=[dynaudnorm=f=150:g=15:p=0.5:m=10:r=0.9:b=1]",
        }
    }
}

/// The equalizer's bands, their centres in Hz.
pub const EQ_BANDS: [u32; 10] = [31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];
/// The most a band is raised or lowered, in dB.
pub const EQ_RANGE: i64 = 12;

/// The ten-band equalizer: a peaking filter an octave wide at each band
/// with a gain (FFmpeg's `equalizer`); kept in the settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Equalizer {
    pub on: bool,
    /// In dB, by [`EQ_BANDS`].
    pub gains: [i64; 10],
}

/// A preset of the equalizer: its gains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Flat,
    Rock,
    Pop,
    Jazz,
    Classical,
    Bass,
    Voice,
    Treble,
}

impl Preset {
    pub const ALL: [Preset; 8] = [
        Preset::Flat,
        Preset::Rock,
        Preset::Pop,
        Preset::Jazz,
        Preset::Classical,
        Preset::Bass,
        Preset::Voice,
        Preset::Treble,
    ];

    pub fn gains(self) -> [i64; 10] {
        match self {
            Preset::Flat => [0; 10],
            Preset::Rock => [5, 4, 3, 1, -1, -1, 1, 3, 4, 5],
            Preset::Pop => [-1, 1, 3, 4, 3, 0, -1, -1, 0, 1],
            Preset::Jazz => [3, 2, 1, 2, -1, -1, 0, 1, 2, 3],
            Preset::Classical => [4, 3, 2, 1, -1, -1, 0, 2, 3, 4],
            Preset::Bass => [7, 6, 5, 3, 1, 0, 0, 0, 0, 0],
            Preset::Voice => [-3, -2, -1, 1, 4, 4, 3, 1, 0, -2],
            Preset::Treble => [0, 0, 0, 0, 0, 1, 2, 4, 5, 6],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Preset::Flat => tr!("Flat", "Ровно"),
            Preset::Rock => tr!("Rock", "Рок"),
            Preset::Pop => tr!("Pop", "Поп"),
            Preset::Jazz => tr!("Jazz", "Джаз"),
            Preset::Classical => tr!("Classical", "Классика"),
            Preset::Bass => tr!("Bass", "Басы"),
            Preset::Voice => tr!("Voice", "Голос"),
            Preset::Treble => tr!("Treble", "Высокие"),
        }
    }

    /// The preset `gains` are, if any.
    pub fn of(gains: &[i64; 10]) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.gains() == *gains)
    }
}

/// Gains the user saved under a name.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnEq {
    pub name: String,
    pub gains: [i64; 10],
}

/// The most own gains kept; saving one more drops the oldest.
pub const MAX_OWN_EQ: usize = 20;

impl OwnEq {
    /// Keep `gains` as `name` in `list`: in place of one of that name,
    /// else at the end.
    pub fn save(list: &mut Vec<OwnEq>, name: &str, gains: [i64; 10]) {
        let name = crate::sub_style::clean(name);
        if name.is_empty() {
            return;
        }
        match list.iter_mut().find(|o| o.name == name) {
            Some(own) => own.gains = gains,
            None => {
                if list.len() >= MAX_OWN_EQ {
                    list.remove(0);
                }
                list.push(OwnEq { name, gains });
            }
        }
    }

    /// The setting: a line each, `name<TAB>g,g,…`.
    pub fn to_text(list: &[OwnEq]) -> String {
        list.iter()
            .map(|o| {
                let gains: Vec<String> = o.gains.iter().map(i64::to_string).collect();
                format!("{}\t{}", o.name, gains.join(","))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The gains of the setting; lines not understood are left out.
    pub fn from_text(text: &str) -> Vec<OwnEq> {
        text.lines()
            .filter_map(|line| {
                let (name, gains) = line.split_once('\t')?;
                let name = crate::sub_style::clean(name);
                let gains: Vec<i64> = gains
                    .split(',')
                    .map(|g| {
                        g.trim()
                            .parse::<i64>()
                            .ok()
                            .map(|g| g.clamp(-EQ_RANGE, EQ_RANGE))
                    })
                    .collect::<Option<_>>()?;
                let gains = <[i64; 10]>::try_from(gains).ok()?;
                (!name.is_empty()).then_some(OwnEq { name, gains })
            })
            .take(MAX_OWN_EQ)
            .collect()
    }
}

/// The band's label: `31`, `1k`, `16k`.
pub fn band_label(hz: u32) -> String {
    if hz >= 1000 {
        format!("{}k", hz / 1000)
    } else {
        hz.to_string()
    }
}

/// How much clearer speech raises the voice over the rest, in dB
/// ([`centre_filter`]; in stereo, `dialoguenhance`'s factor first, its 1
/// at 4 dB).
pub const VOICE_RANGE: std::ops::RangeInclusive<i64> = 1..=20;
/// What it was before it could be set: the centre raised 1.6 times.
pub const VOICE_DEFAULT: i64 = 4;
/// The most the centre itself is raised, in dB; past it the other
/// channels are lowered instead, so that a loud voice does not clip
/// (6 dB was not enough voice, far from clipping).
const CENTRE_MAX_DB: i64 = 12;
/// `dialoguenhance`'s strongest factor, reached at 12 dB.
const ENHANCE_MAX: f64 = 3.0;

/// The voice `db` over the rest of a layout whose third channel is the
/// centre: the centre up to [`CENTRE_MAX_DB`], the others down by what is
/// left (one expression for every channel).
fn centre_filter(db: i64) -> String {
    let up = db.min(CENTRE_MAX_DB);
    let gain = |db: i64| 10f64.powf(db as f64 / 20.0);
    let (centre, others) = (gain(up), gain(up - db));
    format!("lavfi=[aeval=val(ch)*if(eq(ch\\,2)\\,{centre:.2}\\,{others:.2}):c=same]")
}

/// mpv's names of the layouts whose third channel is the centre (FL, FR,
/// FC, …); others name it elsewhere or not at all (`5.1(alsa)`, `quad`,
/// `6.0(front)`, `2.1`).
const CENTRE_THIRD: [&str; 22] = [
    "3.0",
    "3.1",
    "4.0",
    "4.1",
    "5.0",
    "5.0(side)",
    "5.1",
    "5.1(side)",
    "6.0",
    "6.1",
    "6.1(back)",
    "6.1(top)",
    "7.0",
    "7.0(front)",
    "7.0(rear)",
    "7.1",
    "7.1(wide)",
    "7.1(wide-side)",
    "7.1(top)",
    "7.1(rear)",
    "hexagonal",
    "octagonal",
];

/// The filter that makes speech clearer in a track of channel `layout`
/// (mpv's name, `5.1(side)`, or its speakers, `fl-fr-fc-lfe`): stereo
/// through FFmpeg's `dialoguenhance` (it finds the voice common to both
/// sides and puts it in a centre, which the device's mix brings back),
/// a layout with a centre third by raising that channel, whatever the
/// count the decoder gives (one expression for every channel: a list of
/// one per channel failed when the count differed from the track's).
/// `None` for mono, where the voice is all there is, and for layouts
/// without a centre third (an AC-3 2/2.1 has the LFE there). `db`: how
/// much ([`VOICE_RANGE`]).
pub fn voice_filter(layout: &str, db: i64) -> Option<String> {
    let db = db.clamp(*VOICE_RANGE.start(), *VOICE_RANGE.end());
    let speakers: Vec<&str> = layout.split('-').collect();
    if layout == "stereo" || speakers == ["fl", "fr"] {
        // Its 1 for the default; past its 3, the 3.0 it makes gets the
        // rest as a centre raised over its sides.
        let enhance = (db as f64 / VOICE_DEFAULT as f64).min(ENHANCE_MAX);
        let rest = db - (ENHANCE_MAX * VOICE_DEFAULT as f64) as i64;
        let mut filter = format!("lavfi=[dialoguenhance=enhance={enhance:.2}]");
        if rest > 0 {
            filter.push(',');
            filter.push_str(&centre_filter(rest));
        }
        return Some(filter);
    }
    let centre = CENTRE_THIRD.contains(&layout) || speakers.get(2) == Some(&"fc");
    centre.then(|| centre_filter(db))
}

/// The track is stereo, by its layout or its channel count: what Stereo to
/// 5.1 takes.
pub fn stereo_track(track: &crate::tracks::Track) -> bool {
    voice_layout(track.layout.as_deref(), track.channels).is_some_and(|l| is_stereo(&l))
}

/// What keeps the raised voice under full scale when nothing else does:
/// FFmpeg's limiter, without its own levelling of the output.
const LIMITER: &str = "lavfi=[alimiter=limit=0.97:level=0]";

/// The layout a track's voice filter is made for: mpv's, else one its
/// channel count names.
pub fn voice_layout(layout: Option<&str>, channels: Option<i64>) -> Option<String> {
    layout.map(str::to_owned).or_else(|| match channels {
        Some(1) => Some("mono".to_owned()),
        Some(2) => Some("stereo".to_owned()),
        _ => None,
    })
}

/// Where the subwoofer's sound ends, in Hz: FFmpeg's `lfe_low` (its own
/// default, 128, is high for a subwoofer; cinemas cross over at 80).
pub const LFE_CUT_RANGE: std::ops::RangeInclusive<i64> = 40..=200;
/// The subwoofer's level against the standard one, in dB.
pub const LFE_LEVEL_RANGE: std::ops::RangeInclusive<i64> = -20..=10;
/// The standard level of a 5.1's LFE channel: 10 dB under the others,
/// since receivers play it 10 dB louder. `surround` puts the low end there
/// at full level, and the bass boomed.
const LFE_STANDARD_DB: i64 = -10;

/// Stereo spread over 5.1, as the user set it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Upmix {
    /// Below it the low end goes to the subwoofer ([`LFE_CUT_RANGE`]); it
    /// fades out up to twice that.
    pub lfe_cut: i64,
    /// The low end taken out of the other channels (`lfe_mode=sub`), not
    /// only copied to the subwoofer: for small speakers.
    pub bass_only: bool,
}

impl Default for Upmix {
    fn default() -> Self {
        Self {
            lfe_cut: 80,
            bass_only: false,
        }
    }
}

impl Upmix {
    /// The filter: FFmpeg's `surround` finds, band by band, what is common
    /// to both sides (the centre, mostly the voice), what differs (the
    /// surrounds) and the low end (the LFE), at `level`
    /// ([`Subwoofer::level`]; 0 is [`LFE_STANDARD_DB`] in the filter).
    pub fn filter(self, level: i64) -> String {
        let cut = self
            .lfe_cut
            .clamp(*LFE_CUT_RANGE.start(), *LFE_CUT_RANGE.end());
        let mode = if self.bass_only { "sub" } else { "add" };
        let level = level.clamp(*LFE_LEVEL_RANGE.start(), *LFE_LEVEL_RANGE.end());
        let gain = 10f64.powf((level + LFE_STANDARD_DB) as f64 / 20.0);
        format!(
            "lavfi=[surround=chl_out=5.1:lfe_low={cut}:lfe_high={}:lfe_mode={mode}:lfe_out={gain:.3}]",
            cut * 2
        )
    }
}

/// A layout the upmix takes: stereo.
pub fn is_stereo(layout: &str) -> bool {
    layout == "stereo" || layout == "fl-fr"
}

/// A layout with a subwoofer channel: mpv's name `5.1`, `7.1(wide)`, or
/// its speakers, `fl-fr-fc-lfe-…`.
pub fn has_lfe(layout: &str) -> bool {
    let name = layout.split('(').next().unwrap_or(layout);
    name.split('.').nth(1) == Some("1") || layout.split('-').any(|s| s == "lfe")
}

/// Where the cut of the deepest bass may be set, in Hz.
pub const INFRASONIC_RANGE: std::ops::RangeInclusive<i64> = 15..=40;

/// The subwoofer, for every track that has one (or gets one from the
/// upmix).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Subwoofer {
    /// Its level in dB ([`LFE_LEVEL_RANGE`]): 0 leaves a track's LFE as it
    /// is and gives the upmix's the standard level.
    pub level: i64,
    /// What lies under `infrasonic_hz` taken out of all the channels: a
    /// subwoofer hardly plays it, but its cone is driven to its limits and
    /// the bass bubbles (a film's LFE can hold as much under 25 Hz as
    /// above).
    pub infrasonic: bool,
    pub infrasonic_hz: i64,
}

impl Default for Subwoofer {
    fn default() -> Self {
        Self {
            level: 0,
            infrasonic: false,
            infrasonic_hz: 25,
        }
    }
}

impl Subwoofer {
    /// Its filters for a track with an LFE channel (`lfe`) or without, the
    /// upmix aside (its level is the upmix's own): one graph.
    fn filter(self, lfe: bool) -> Option<String> {
        let mut parts = Vec::new();
        let level = self
            .level
            .clamp(*LFE_LEVEL_RANGE.start(), *LFE_LEVEL_RANGE.end());
        if lfe && level != 0 {
            // A biquad of a gain alone: FFmpeg's `volume` takes every
            // channel, the biquads take `c`.
            let gain = 10f64.powf(level as f64 / 20.0);
            parts.push(format!(
                "biquad=b0={gain:.3}:b1=0:b2=0:a0=1:a1=0:a2=0:c=LFE"
            ));
        }
        if self.infrasonic {
            let hz = self
                .infrasonic_hz
                .clamp(*INFRASONIC_RANGE.start(), *INFRASONIC_RANGE.end());
            // Two Butterworths: 24 dB an octave, half at `hz`.
            parts.extend([
                format!("highpass=f={hz}:p=2"),
                format!("highpass=f={hz}:p=2"),
            ]);
        }
        (!parts.is_empty()).then(|| format!("lavfi=[{}]", parts.join(",")))
    }
}

/// mpv's `af`: stereo spread over 5.1 when `upmix` is given (for a
/// stereo track, [`Upmix::filter`]), the subwoofer's level (for a track
/// with an LFE channel, `lfe`) and the cut under it ([`Subwoofer`]), the
/// voice's filter when `voice` gives the channel layout of the track
/// playing and the strength ([`voice_filter`]; after the upmix, the
/// 5.1's), the equalizer's bands that are not flat (one graph), then the
/// normalization (which evens out what the others raised).
pub fn filters(
    normalize: Option<Normalize>,
    eq: &Equalizer,
    voice: Option<(&str, i64)>,
    upmix: Option<Upmix>,
    sub: Subwoofer,
    lfe: bool,
) -> String {
    let mut af: Vec<String> = Vec::new();
    if let Some(upmix) = upmix {
        af.push(upmix.filter(sub.level));
    }
    let lfe_raised = upmix.is_none() && lfe && sub.level > 0;
    if let Some(filter) = sub.filter(lfe && upmix.is_none()) {
        af.push(filter);
    }
    let voice = voice.map(|(layout, db)| (if upmix.is_some() { "5.1" } else { layout }, db));
    let voice = voice.and_then(|(layout, db)| voice_filter(layout, db));
    if let Some(filter) = &voice {
        af.push(filter.clone());
    }
    // A voice raised 12 dB goes over full scale where it was over -12 dB,
    // as a raised LFE may; the normalization keeps the peaks under, else
    // a limiter.
    if (voice.is_some() || lfe_raised) && normalize.is_none() {
        af.push(LIMITER.to_owned());
    }
    if eq.on {
        // One graph for all the bands: one filter for mpv to set up.
        let bands: Vec<String> = EQ_BANDS
            .iter()
            .zip(eq.gains)
            .filter(|(_, gain)| *gain != 0)
            .map(|(hz, gain)| {
                let gain = gain.clamp(-EQ_RANGE, EQ_RANGE);
                format!("equalizer=f={hz}:t=o:w=1:g={gain}")
            })
            .collect();
        if !bands.is_empty() {
            af.push(format!("lavfi=[{}]", bands.join(",")));
        }
    }
    if let Some(normalize) = normalize {
        af.push(normalize.filter().to_owned());
    }
    af.join(",")
}

/// The channels sent to the device.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Channels {
    /// As the device is set up in Windows; more channels are mixed down.
    #[default]
    Auto,
    Mono,
    Stereo,
    Surround51,
    Surround71,
}

impl Channels {
    pub const ALL: [Channels; 5] = [
        Channels::Auto,
        Channels::Mono,
        Channels::Stereo,
        Channels::Surround51,
        Channels::Surround71,
    ];

    /// The stored name.
    pub fn name(self) -> &'static str {
        match self {
            Channels::Auto => "auto",
            Channels::Mono => "mono",
            Channels::Stereo => "stereo",
            Channels::Surround51 => "5.1",
            Channels::Surround71 => "7.1",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }

    /// Stereo can be spread over 5.1 for it: not when the sound is made
    /// mono or stereo anyway.
    pub fn takes_upmix(self) -> bool {
        !matches!(self, Channels::Mono | Channels::Stereo)
    }

    /// mpv's `audio-channels`.
    pub fn option(self) -> &'static str {
        match self {
            Channels::Auto => "auto-safe",
            other => other.name(),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Channels::Auto => tr!("Auto", "Авто"),
            Channels::Mono => tr!("Mono", "Моно"),
            Channels::Stereo => tr!("Stereo", "Стерео"),
            Channels::Surround51 => "5.1",
            Channels::Surround71 => "7.1",
        }
    }
}

/// Compressed sound sent as it is to a receiver, which decodes it
/// (`audio-spdif`). WASAPI takes it in exclusive mode only, which mpv
/// asks for itself; a format the device refuses is decoded as usual.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Passthrough {
    #[default]
    Off,
    /// What an optical or coaxial cable carries.
    Spdif,
    /// HDMI carries the lossless formats too.
    Hdmi,
}

impl Passthrough {
    pub const ALL: [Passthrough; 3] = [Passthrough::Off, Passthrough::Spdif, Passthrough::Hdmi];

    /// The stored name.
    pub fn name(self) -> &'static str {
        match self {
            Passthrough::Off => "off",
            Passthrough::Spdif => "spdif",
            Passthrough::Hdmi => "hdmi",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }

    /// mpv's `audio-spdif`: the codecs passed through.
    pub fn option(self) -> &'static str {
        match self {
            Passthrough::Off => "",
            Passthrough::Spdif => "ac3,dts",
            Passthrough::Hdmi => "ac3,eac3,dts,dts-hd,truehd",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Passthrough::Off => tr!("Off", "Выкл."),
            Passthrough::Spdif => tr!("S/PDIF: AC3, DTS", "S/PDIF: AC3, DTS"),
            Passthrough::Hdmi => tr!(
                "HDMI: AC3, E-AC3, DTS, DTS-HD, TrueHD",
                "HDMI: AC3, E-AC3, DTS, DTS-HD, TrueHD"
            ),
        }
    }
}

/// The label of a volume ceiling.
pub fn boost_label(max: f64) -> String {
    if max <= 100.0 {
        tr!("None (up to 100%)", "Нет (до 100%)").into()
    } else {
        tr!(format!("Up to {max:.0}%"), format!("До {max:.0}%"))
    }
}

/// An output device as mpv names it.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    /// mpv's `audio-device` value ("wasapi/{…}").
    pub name: String,
    /// What Windows calls it ("Speakers (Realtek(R) Audio)").
    pub description: String,
}

/// The WASAPI devices of mpv's `audio-device-list`: Windows' own. The
/// list also has `auto` and other drivers' (`openal`), which choosing
/// would switch the driver.
pub fn devices_from_value(list: &Value) -> Vec<Device> {
    list.list()
        .iter()
        .filter_map(|d| {
            let name = d.get("name")?.string()?;
            if !name.starts_with("wasapi/") {
                return None;
            }
            let description = d
                .get("description")
                .and_then(Value::string)
                .filter(|s| !s.is_empty())
                .unwrap_or(name);
            Some(Device {
                name: name.to_owned(),
                description: description.to_owned(),
            })
        })
        .collect()
}

/// The device to give mpv: the chosen one while Windows has it, else
/// Windows' default (empty). mpv does not fall back from a device that is
/// gone (ao.c's `forced_dev`): the sound would be lost. `None` while the
/// devices are not known yet.
pub fn device_to_use<'a>(chosen: &'a str, devices: &[Device]) -> Option<&'a str> {
    if devices.is_empty() {
        return None;
    }
    Some(if devices.iter().any(|d| d.name == chosen) {
        chosen
    } else {
        ""
    })
}

/// What the player is started with (the device follows once mpv lists
/// the devices, [`device_to_use`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Setup {
    pub volume: f64,
    pub mute: bool,
    /// The volume's ceiling, 100 to [`MAX_VOLUME`].
    pub volume_max: f64,
    pub channels: Channels,
    pub normalize: Option<Normalize>,
    pub equalizer: Equalizer,
    pub passthrough: Passthrough,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_names_round_trip() {
        for c in Channels::ALL {
            assert_eq!(Channels::from_name(c.name()), Some(c));
        }
        assert_eq!(Channels::Auto.option(), "auto-safe");
        assert_eq!(Channels::Surround51.option(), "5.1");
        assert_eq!(Channels::from_name("9.1"), None);
        for p in Passthrough::ALL {
            assert_eq!(Passthrough::from_name(p.name()), Some(p));
        }
        assert_eq!(Passthrough::Off.option(), "");
        assert_eq!(Passthrough::Spdif.option(), "ac3,dts");
    }

    #[test]
    fn equalizer_filters() {
        let mut eq = Equalizer::default();
        assert_eq!(
            filters(None, &eq, None, None, Subwoofer::default(), false),
            ""
        );
        eq.gains = Preset::Bass.gains();
        assert_eq!(
            filters(None, &eq, None, None, Subwoofer::default(), false),
            "",
            "off: no bands"
        );
        eq.on = true;
        eq.gains = [0, 0, 0, 0, 0, 0, 0, 0, 3, -20];
        assert_eq!(
            filters(
                Some(Normalize::Medium),
                &eq,
                None,
                None,
                Subwoofer::default(),
                false
            ),
            format!(
                "lavfi=[equalizer=f=8000:t=o:w=1:g=3,equalizer=f=16000:t=o:w=1:g=-12],{}",
                Normalize::Medium.filter()
            )
        );
        assert_eq!(Preset::of(&Preset::Rock.gains()), Some(Preset::Rock));
        assert_eq!(band_label(16000), "16k");
        assert_eq!(band_label(62), "62");
    }

    #[test]
    fn own_gains() {
        let mut list = Vec::new();
        OwnEq::save(&mut list, " Night ", [1, 2, 3, 4, 5, 6, 7, 8, 9, -12]);
        OwnEq::save(&mut list, "Night", [0; 10]);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].gains, [0; 10]);
        OwnEq::save(&mut list, "Loud", [12; 10]);
        let text = OwnEq::to_text(&list);
        assert_eq!(
            text,
            "Night\t0,0,0,0,0,0,0,0,0,0\nLoud\t12,12,12,12,12,12,12,12,12,12"
        );
        assert_eq!(OwnEq::from_text(&text), list);
        assert_eq!(
            OwnEq::from_text("Odd\t1,2,3\nBig\t99,0,0,0,0,0,0,0,0,0")[0].gains[0],
            12
        );
    }

    #[test]
    fn voice_filters() {
        let centre = "lavfi=[aeval=val(ch)*if(eq(ch\\,2)\\,1.58\\,1.00):c=same]";
        // Past 12 dB the others go down instead: 16 dB is the centre +12,
        // the rest -4.
        assert_eq!(
            voice_filter("5.1", 16).as_deref(),
            Some("lavfi=[aeval=val(ch)*if(eq(ch\\,2)\\,3.98\\,0.63):c=same]")
        );
        // Stereo past the enhancer's 3 (12 dB): the rest on its centre.
        assert_eq!(
            voice_filter("stereo", 20).as_deref(),
            Some(
                "lavfi=[dialoguenhance=enhance=3.00],lavfi=[aeval=val(ch)*if(eq(ch\\,2)\\,2.51\\,1.00):c=same]"
            )
        );
        assert_eq!(
            voice_filter("stereo", 12).as_deref(),
            Some("lavfi=[dialoguenhance=enhance=3.00]")
        );
        assert_eq!(voice_filter("mono", VOICE_DEFAULT), None);
        assert_eq!(voice_filter("2.1", VOICE_DEFAULT), None);
        assert_eq!(voice_filter("quad", VOICE_DEFAULT), None);
        assert_eq!(voice_filter("5.1(alsa)", VOICE_DEFAULT), None);
        // AC-3 2/2.1: the LFE third.
        assert_eq!(voice_filter("fl-fr-lfe-sl-sr", VOICE_DEFAULT), None);
        for layout in ["stereo", "fl-fr"] {
            assert_eq!(
                voice_filter(layout, VOICE_DEFAULT).as_deref(),
                Some("lavfi=[dialoguenhance=enhance=1.00]")
            );
        }
        for layout in ["5.1", "5.1(side)", "7.1", "fl-fr-fc-lfe-sl-sr", "3.0"] {
            assert_eq!(
                voice_filter(layout, VOICE_DEFAULT).as_deref(),
                Some(centre),
                "{layout}"
            );
        }
        assert_eq!(voice_layout(None, Some(2)).as_deref(), Some("stereo"));
        assert_eq!(voice_layout(Some("5.1"), Some(6)).as_deref(), Some("5.1"));
        assert_eq!(voice_layout(None, Some(6)), None);
        let eq = Equalizer::default();
        assert_eq!(
            filters(
                Some(Normalize::Medium),
                &eq,
                Some(("stereo", VOICE_DEFAULT)),
                None,
                Subwoofer::default(),
                false
            ),
            format!(
                "lavfi=[dialoguenhance=enhance=1.00],{}",
                Normalize::Medium.filter()
            )
        );
        // Spread over 5.1, the voice is raised in its centre.
        let upmix = Upmix::default();
        assert_eq!(
            upmix.filter(0),
            "lavfi=[surround=chl_out=5.1:lfe_low=80:lfe_high=160:lfe_mode=add:lfe_out=0.316]"
        );
        assert_eq!(
            filters(
                None,
                &eq,
                Some(("stereo", VOICE_DEFAULT)),
                Some(upmix),
                Subwoofer::default(),
                false
            ),
            format!("{},{centre},{LIMITER}", upmix.filter(0))
        );
        let small = Upmix {
            lfe_cut: 500,
            bass_only: true,
        };
        assert_eq!(
            small.filter(10),
            "lavfi=[surround=chl_out=5.1:lfe_low=200:lfe_high=400:lfe_mode=sub:lfe_out=1.000]"
        );
        assert!(is_stereo("stereo") && is_stereo("fl-fr") && !is_stereo("5.1"));
        assert!(Channels::Auto.takes_upmix() && !Channels::Stereo.takes_upmix());
    }

    #[test]
    fn subwoofer_filters() {
        for layout in ["5.1", "5.1(side)", "7.1(wide)", "2.1", "fl-fr-lfe-sl-sr"] {
            assert!(has_lfe(layout), "{layout}");
        }
        for layout in ["stereo", "5.0(side)", "7.0", "quad", "mono"] {
            assert!(!has_lfe(layout), "{layout}");
        }
        let eq = Equalizer::default();
        let sub = Subwoofer {
            level: -6,
            infrasonic: true,
            infrasonic_hz: 25,
        };
        let cut = "highpass=f=25:p=2,highpass=f=25:p=2";
        assert_eq!(
            filters(None, &eq, None, None, sub, true),
            format!("lavfi=[biquad=b0=0.501:b1=0:b2=0:a0=1:a1=0:a2=0:c=LFE,{cut}]")
        );
        // No LFE: only the cut; nothing when it is off and the level is 0.
        assert_eq!(
            filters(None, &eq, None, None, sub, false),
            format!("lavfi=[{cut}]")
        );
        assert_eq!(
            filters(None, &eq, None, None, Subwoofer::default(), true),
            ""
        );
        // The upmix's LFE gets the level in `surround`, the cut after it.
        let upmix = Upmix::default();
        assert_eq!(
            filters(None, &eq, None, Some(upmix), sub, false),
            format!("{},lavfi=[{cut}]", upmix.filter(-6))
        );
        // A raised LFE is kept under full scale.
        let loud = Subwoofer {
            level: 4,
            ..Subwoofer::default()
        };
        assert_eq!(
            filters(None, &eq, None, None, loud, true),
            format!("lavfi=[biquad=b0=1.585:b1=0:b2=0:a0=1:a1=0:a2=0:c=LFE],{LIMITER}")
        );
    }

    #[test]
    fn a_missing_device_falls_back() {
        let devices = [Device {
            name: "wasapi/{1}".into(),
            description: "Speakers".into(),
        }];
        assert_eq!(device_to_use("wasapi/{1}", &devices), Some("wasapi/{1}"));
        assert_eq!(device_to_use("wasapi/{9}", &devices), Some(""));
        assert_eq!(device_to_use("", &devices), Some(""));
        assert_eq!(device_to_use("wasapi/{1}", &[]), None);
    }

    #[test]
    fn only_wasapi_devices() {
        let device = |name: &str, description: &str| {
            Value::Map(vec![
                ("name".into(), Value::Str(name.into())),
                ("description".into(), Value::Str(description.into())),
            ])
        };
        let list = Value::List(vec![
            device("auto", "Autoselect device"),
            device("openal", "Default (openal)"),
            device("wasapi/{1}", "Speakers (Realtek(R) Audio)"),
            device("wasapi/{2}", ""),
        ]);
        assert_eq!(
            devices_from_value(&list),
            vec![
                Device {
                    name: "wasapi/{1}".into(),
                    description: "Speakers (Realtek(R) Audio)".into()
                },
                Device {
                    name: "wasapi/{2}".into(),
                    description: "wasapi/{2}".into()
                },
            ]
        );
    }
}
