//! The keys of the player as the user set them: the actions a key can
//! do, the keys (`Chord`s) each has by default, the user's changes kept
//! in the settings, and the labels the menus and tooltips show.
//!
//! Only the changes are kept (`Keymap::to_setting`): an action not named
//! there has its default keys, so a new action of a later version gets
//! its keys even where others were changed.

use std::sync::{LazyLock, RwLock};

use egui::{Key, Modifiers, PointerButton};

use crate::adjust::Color;
use crate::input::{Cmd, VOLUME_STEP};

/// What a chord presses: a key, or a side button of the mouse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Press {
    Key(Key),
    /// The mouse's back button (egui's `Extra1`).
    MouseBack,
    /// The mouse's forward button (egui's `Extra2`).
    MouseForward,
}

/// A key or a side button with the modifiers held.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub press: Press,
}

/// `m` with Ctrl: a copy or cut event comes from a press with it.
fn with_ctrl(m: Modifiers) -> Modifiers {
    Modifiers { ctrl: true, ..m }
}

impl Chord {
    const fn key(key: Key) -> Self {
        Self::of(Press::Key(key), false, false, false)
    }

    const fn ctrl(key: Key) -> Self {
        Self::of(Press::Key(key), true, false, false)
    }

    const fn shift(key: Key) -> Self {
        Self::of(Press::Key(key), false, false, true)
    }

    const fn alt(key: Key) -> Self {
        Self::of(Press::Key(key), false, true, false)
    }

    const fn of(press: Press, ctrl: bool, alt: bool, shift: bool) -> Self {
        Self {
            ctrl,
            alt,
            shift,
            press,
        }
    }

    pub fn new(press: Press, m: Modifiers) -> Self {
        Self::of(press, m.command || m.ctrl, m.alt, m.shift)
    }

    /// The chord of a key press, unless the key is a modifier itself
    /// (egui-winit passes the physical Shift, Ctrl and Alt as keys).
    pub fn of_key(key: Key, m: Modifiers) -> Option<Self> {
        let modifier = matches!(
            key,
            Key::ShiftLeft
                | Key::ShiftRight
                | Key::ControlLeft
                | Key::ControlRight
                | Key::AltLeft
                | Key::AltRight
                | Key::SuperLeft
                | Key::SuperRight
        );
        (!modifier).then(|| Self::new(Press::Key(key), m))
    }

    /// The chord of a press of the mouse's side button.
    pub fn of_button(button: PointerButton, m: Modifiers) -> Option<Self> {
        let press = match button {
            PointerButton::Extra1 => Press::MouseBack,
            PointerButton::Extra2 => Press::MouseForward,
            _ => return None,
        };
        Some(Self::new(press, m))
    }

    /// The chord of a press in `event`, `modifiers` held, and whether it is
    /// a held key's repeat. egui-winit turns `Ctrl+C` and `Ctrl+X` (with
    /// any other modifiers), `Ctrl+Insert` and `Shift+Delete` into `Copy`
    /// and `Cut` with no key event: the modifiers held tell them apart, but
    /// for `Ctrl+Insert`, taken for `Ctrl+C`. `Ctrl+V` and `Shift+Insert`
    /// become a paste of the clipboard's text, so they are no keys here.
    pub fn of_event(event: &egui::Event, modifiers: egui::Modifiers) -> Option<(Self, bool)> {
        use egui::Event;
        match event {
            Event::Key {
                key,
                pressed: true,
                repeat,
                modifiers,
                ..
            } => Some((Self::of_key(*key, *modifiers)?, *repeat)),
            Event::PointerButton {
                button,
                pressed: true,
                modifiers,
                ..
            } => Some((Self::of_button(*button, *modifiers)?, false)),
            Event::Copy => Some((Self::new(Press::Key(Key::C), with_ctrl(modifiers)), false)),
            // Shift+Delete, or Ctrl+X.
            Event::Cut if modifiers.shift && !modifiers.ctrl => {
                Some((Self::new(Press::Key(Key::Delete), modifiers), false))
            }
            Event::Cut => Some((Self::new(Press::Key(Key::X), with_ctrl(modifiers)), false)),
            _ => None,
        }
    }

    /// As kept in the settings: `Ctrl+Shift+Left`, `Alt+MouseBack`.
    pub fn name(&self) -> String {
        let press = match self.press {
            Press::Key(key) => key.name(),
            Press::MouseBack => "MouseBack",
            Press::MouseForward => "MouseForward",
        };
        self.with_modifiers(press)
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let mut chord = Self::key(Key::Space);
        let mut press = None;
        for part in name.split('+') {
            match part {
                "Ctrl" => chord.ctrl = true,
                "Alt" => chord.alt = true,
                "Shift" => chord.shift = true,
                "MouseBack" => press = Some(Press::MouseBack),
                "MouseForward" => press = Some(Press::MouseForward),
                other => press = Some(Press::Key(Key::from_name(other)?)),
            }
        }
        chord.press = press?;
        Some(chord)
    }

    /// As shown to the user: `Ctrl+←`, `Пробел`.
    pub fn label(&self) -> String {
        let press = match self.press {
            Press::Key(key) => key_label(key),
            Press::MouseBack => tr!("Mouse back", "Мышь назад"),
            Press::MouseForward => tr!("Mouse forward", "Мышь вперёд"),
        };
        self.with_modifiers(press)
    }

    fn with_modifiers(&self, press: &str) -> String {
        let mut s = String::new();
        for (on, name) in [
            (self.ctrl, "Ctrl+"),
            (self.alt, "Alt+"),
            (self.shift, "Shift+"),
        ] {
            if on {
                s.push_str(name);
            }
        }
        s.push_str(press);
        s
    }
}

/// A key's name on a keycap.
fn key_label(key: Key) -> &'static str {
    match key {
        Key::ArrowLeft => "←",
        Key::ArrowRight => "→",
        Key::ArrowUp => "↑",
        Key::ArrowDown => "↓",
        Key::Space => tr!("Space", "Пробел"),
        Key::Escape => "Esc",
        Key::Tab => "Tab",
        Key::PageUp => "PgUp",
        Key::PageDown => "PgDn",
        Key::Minus => "-",
        // The key left of 1 and the brackets by their symbols in both
        // languages, whatever the layout puts on them.
        Key::Backtick => "`",
        Key::OpenBracket => "[",
        Key::CloseBracket => "]",
        _ => key.symbol_or_name(),
    }
}

/// The parts of the key list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Playback,
    Sound,
    Window,
    Files,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Playback, Group::Sound, Group::Window, Group::Files];

    pub fn title(self) -> &'static str {
        match self {
            Group::Playback => tr!("Playback", "Воспроизведение"),
            Group::Sound => tr!("Sound and subtitles", "Звук и субтитры"),
            Group::Window => tr!("Window and picture", "Окно и картинка"),
            Group::Files => tr!("Files and windows", "Файлы и окна"),
        }
    }
}

macro_rules! actions {
    ($($action:ident $id:literal $group:ident $cmd:expr, [$($chord:expr),*];)*) => {
        /// What a key can do.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Action {
            $($action,)*
        }

        impl Action {
            pub const ALL: &'static [Action] = &[$(Action::$action,)*];

            /// The name in the settings.
            pub fn id(self) -> &'static str {
                match self {
                    $(Action::$action => $id,)*
                }
            }

            pub fn group(self) -> Group {
                match self {
                    $(Action::$action => Group::$group,)*
                }
            }

            pub fn cmd(self) -> Cmd {
                match self {
                    $(Action::$action => $cmd,)*
                }
            }

            pub fn defaults(self) -> &'static [Chord] {
                match self {
                    $(Action::$action => const { &[$($chord),*] },)*
                }
            }
        }
    };
}

/// A colour key's step, of -100..100.
const COLOR_STEP: i64 = 2;
/// A zoom key's step, in percent.
const ZOOM_STEP: i64 = 10;
/// A pan key's step, in percent of the picture.
const PAN_STEP: i64 = 2;

const MOUSE_BACK: Chord = Chord::of(Press::MouseBack, false, false, false);
const MOUSE_FORWARD: Chord = Chord::of(Press::MouseForward, false, false, false);

actions! {
    Pause "pause" Playback Cmd::TogglePause, [Chord::key(Key::Space)];
    Stop "stop" Playback Cmd::Stop, [];
    SeekBack "seek_back" Playback Cmd::SeekShort(-1.0), [Chord::key(Key::ArrowLeft), MOUSE_BACK];
    SeekForward "seek_forward" Playback Cmd::SeekShort(1.0),
        [Chord::key(Key::ArrowRight), MOUSE_FORWARD];
    SeekBackLong "seek_back_long" Playback Cmd::SeekLong(-1.0),
        [Chord::ctrl(Key::ArrowLeft), Chord::of(Press::MouseBack, true, false, false)];
    SeekForwardLong "seek_forward_long" Playback Cmd::SeekLong(1.0),
        [Chord::ctrl(Key::ArrowRight), Chord::of(Press::MouseForward, true, false, false)];
    SeekBackExact "seek_back_exact" Playback Cmd::SeekExact(-1.0),
        [Chord::shift(Key::ArrowLeft), Chord::of(Press::MouseBack, false, false, true)];
    SeekForwardExact "seek_forward_exact" Playback Cmd::SeekExact(1.0),
        [Chord::shift(Key::ArrowRight), Chord::of(Press::MouseForward, false, false, true)];
    FrameBack "frame_back" Playback Cmd::Frame { forward: false },
        [Chord::key(Key::Comma), Chord::key(Key::D)];
    FrameForward "frame_forward" Playback Cmd::Frame { forward: true },
        [Chord::key(Key::Period), Chord::key(Key::F)];
    Start "start" Playback Cmd::Start, [Chord::key(Key::Home)];
    End "end" Playback Cmd::End, [Chord::key(Key::End)];
    PrevChapter "prev_chapter" Playback Cmd::ChapterStep(-1), [Chord::ctrl(Key::PageUp)];
    NextChapter "next_chapter" Playback Cmd::ChapterStep(1), [Chord::ctrl(Key::PageDown)];
    ChapterMarks "chapter_marks" Playback Cmd::ToggleChapterMarks, [];
    Faster "faster" Playback Cmd::Speed(1), [Chord::key(Key::C)];
    Slower "slower" Playback Cmd::Speed(-1), [Chord::key(Key::X)];
    NormalSpeed "normal_speed" Playback Cmd::Speed(0), [Chord::key(Key::Z)];
    Position "position" Playback Cmd::ShowPosition, [Chord::key(Key::Tab)];
    TimeDisplay "time_display" Playback Cmd::CycleTime, [];
    GoToTime "go_to_time" Playback Cmd::GoToTime, [Chord::ctrl(Key::G)];
    AddBookmark "add_bookmark" Playback Cmd::AddBookmark, [Chord::ctrl(Key::B)];
    Repeat "repeat" Playback Cmd::CycleRepeat, [Chord::key(Key::R)];
    LoopA "loop_a" Playback Cmd::LoopA, [Chord::key(Key::OpenBracket)];
    LoopB "loop_b" Playback Cmd::LoopB, [Chord::key(Key::CloseBracket)];
    LoopClear "loop_clear" Playback Cmd::LoopClear, [Chord::key(Key::Backslash)];
    LoopCycle "loop_cycle" Playback Cmd::CycleLoop, [];

    VolumeUp "volume_up" Sound Cmd::Volume(VOLUME_STEP), [Chord::key(Key::ArrowUp)];
    VolumeDown "volume_down" Sound Cmd::Volume(-VOLUME_STEP), [Chord::key(Key::ArrowDown)];
    Mute "mute" Sound Cmd::ToggleMute, [Chord::key(Key::M)];
    Tracks "tracks" Sound Cmd::ToggleTracks, [Chord::key(Key::T)];
    NextAudio "next_audio" Sound Cmd::CycleAudio, [Chord::key(Key::A)];
    NextSubs "next_subs" Sound Cmd::CycleSubs, [Chord::key(Key::S), Chord::key(Key::J)];
    SubsOnOff "subs_on_off" Sound Cmd::ToggleSubs, [Chord::key(Key::V)];
    LoadSubs "load_subs" Sound Cmd::LoadSubtitles, [];
    SubsLater "subs_later" Sound Cmd::SubDelay(1.0), [Chord::shift(Key::Period)];
    SubsEarlier "subs_earlier" Sound Cmd::SubDelay(-1.0), [Chord::shift(Key::Comma)];
    SubsInSync "subs_in_sync" Sound Cmd::SubDelay(0.0), [Chord::shift(Key::Num0)];
    SoundLater "sound_later" Sound Cmd::AudioDelay(1.0),
        [Chord::ctrl(Key::Equals), Chord::ctrl(Key::Plus)];
    SoundEarlier "sound_earlier" Sound Cmd::AudioDelay(-1.0), [Chord::ctrl(Key::Minus)];
    SoundInSync "sound_in_sync" Sound Cmd::AudioDelay(0.0), [Chord::ctrl(Key::Num0)];
    ResetSubStyle "reset_sub_style" Sound Cmd::ResetSubStyle, [];
    ResetEqualizer "reset_equalizer" Sound Cmd::ResetEqualizer, [];
    EqualizerOnOff "equalizer_on_off" Sound Cmd::ToggleEqualizer, [Chord::key(Key::E)];
    Normalize "normalize" Sound Cmd::ToggleNormalize, [Chord::key(Key::N)];
    VoiceBoost "voice_boost" Sound Cmd::ToggleVoiceBoost, [Chord::key(Key::B)];
    Upmix "upmix" Sound Cmd::ToggleUpmix, [Chord::shift(Key::B)];

    Fullscreen "fullscreen" Window Cmd::ToggleFullscreen,
        [Chord::key(Key::Enter), Chord::of(Press::Key(Key::Enter), false, true, false),
         Chord::key(Key::F11)];
    Escape "escape" Window Cmd::Escape, [Chord::key(Key::Escape)];
    Scale50 "scale_50" Window Cmd::WindowScale(0.5), [Chord::key(Key::Num1)];
    Scale100 "scale_100" Window Cmd::WindowScale(1.0), [Chord::key(Key::Num2)];
    Scale150 "scale_150" Window Cmd::WindowScale(1.5), [Chord::key(Key::Num3)];
    Scale200 "scale_200" Window Cmd::WindowScale(2.0), [Chord::key(Key::Num4)];
    FitScreen "fit_screen" Window Cmd::FitScreen, [Chord::key(Key::Num5)];
    FitWindow "fit_window" Window Cmd::FitWindow, [Chord::key(Key::Backtick)];
    Fill "fill" Window Cmd::ToggleFill, [Chord::key(Key::Num6)];
    ResetPicture "reset_picture" Window Cmd::ResetPicture, [Chord::key(Key::Q)];
    Rotate "rotate" Window Cmd::Rotate, [Chord::ctrl(Key::R)];
    Aspect "aspect" Window Cmd::CycleAspect, [Chord::ctrl(Key::A)];
    Deinterlace "deinterlace" Window Cmd::CycleDeinterlace, [Chord::ctrl(Key::D)];
    ContrastDown "contrast_down" Window Cmd::Color(Color::Contrast, -COLOR_STEP),
        [Chord::ctrl(Key::Num1)];
    ContrastUp "contrast_up" Window Cmd::Color(Color::Contrast, COLOR_STEP),
        [Chord::ctrl(Key::Num2)];
    BrightnessDown "brightness_down" Window Cmd::Color(Color::Brightness, -COLOR_STEP),
        [Chord::ctrl(Key::Num3)];
    BrightnessUp "brightness_up" Window Cmd::Color(Color::Brightness, COLOR_STEP),
        [Chord::ctrl(Key::Num4)];
    GammaDown "gamma_down" Window Cmd::Color(Color::Gamma, -COLOR_STEP), [Chord::ctrl(Key::Num5)];
    GammaUp "gamma_up" Window Cmd::Color(Color::Gamma, COLOR_STEP), [Chord::ctrl(Key::Num6)];
    SaturationDown "saturation_down" Window Cmd::Color(Color::Saturation, -COLOR_STEP),
        [Chord::ctrl(Key::Num7)];
    SaturationUp "saturation_up" Window Cmd::Color(Color::Saturation, COLOR_STEP),
        [Chord::ctrl(Key::Num8)];
    ZoomIn "zoom_in" Window Cmd::Zoom(ZOOM_STEP), [Chord::alt(Key::Equals), Chord::alt(Key::Plus)];
    ZoomOut "zoom_out" Window Cmd::Zoom(-ZOOM_STEP), [Chord::alt(Key::Minus)];
    PanLeft "pan_left" Window Cmd::Pan(-PAN_STEP, 0), [Chord::alt(Key::ArrowLeft)];
    PanRight "pan_right" Window Cmd::Pan(PAN_STEP, 0), [Chord::alt(Key::ArrowRight)];
    PanUp "pan_up" Window Cmd::Pan(0, -PAN_STEP), [Chord::alt(Key::ArrowUp)];
    PanDown "pan_down" Window Cmd::Pan(0, PAN_STEP), [Chord::alt(Key::ArrowDown)];
    ZoomReset "zoom_reset" Window Cmd::ZoomReset, [Chord::alt(Key::Backspace)];
    Interface "interface" Window Cmd::ToggleInterface, [Chord::key(Key::Num7)];
    Interfaces "interfaces" Window Cmd::CycleInterface, [Chord::key(Key::Num8)];
    OnTop "on_top" Window Cmd::CycleOnTop, [Chord::ctrl(Key::T)];
    Minimize "minimize" Window Cmd::Minimize, [];
    Maximize "maximize" Window Cmd::ToggleMaximize, [];

    Prev "prev" Files Cmd::Prev, [Chord::key(Key::PageUp)];
    Next "next" Files Cmd::Next, [Chord::key(Key::PageDown)];
    Open "open" Files Cmd::Open, [Chord::ctrl(Key::O)];
    OpenUrl "open_url" Files Cmd::OpenUrl, [Chord::ctrl(Key::U)];
    Playlist "playlist" Files Cmd::TogglePlaylist, [Chord::key(Key::L)];
    PlaylistWindow "playlist_window" Files Cmd::TogglePlaylistWindow, [];
    Delete "delete" Files Cmd::Delete, [Chord::key(Key::Delete)];
    PlaylistAdd "playlist_add" Files Cmd::PlaylistAdd, [Chord::key(Key::Insert)];
    PlaylistSave "playlist_save" Files Cmd::PlaylistSave, [Chord::ctrl(Key::S)];
    PlaylistShuffle "playlist_shuffle" Files Cmd::PlaylistShuffle, [];
    PlaylistSort "playlist_sort" Files Cmd::PlaylistSort, [];
    PlaylistClear "playlist_clear" Files Cmd::PlaylistClear, [];
    ShowInFolder "show_in_folder" Files Cmd::ShowInFolder, [];
    Info "info" Files Cmd::ToggleInfo, [Chord::key(Key::I)];
    VideoPanel "video_panel" Files Cmd::ToggleVideoPanel, [Chord::key(Key::U), Chord::key(Key::F7)];
    Equalizer "equalizer" Files Cmd::ShowEqualizer, [];
    SaveFrame "save_frame" Files Cmd::SaveFrame, [Chord::ctrl(Key::E)];
    ShowFrames "show_frames" Files Cmd::ShowFrames, [];
    SaveCover "save_cover" Files Cmd::SaveCover, [];
    Settings "settings" Files Cmd::Settings, [Chord::key(Key::F5), Chord::ctrl(Key::Comma)];
    About "about" Files Cmd::About, [Chord::key(Key::F1)];
    Close "close" Files Cmd::Close, [];
}

impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Action::Pause => tr!("Play / pause", "Воспроизведение / пауза"),
            Action::Stop => tr!("Stop", "Стоп"),
            Action::SeekBack => tr!(
                "Back by the short step (keyframes)",
                "Назад на короткий шаг (по ключевым кадрам)"
            ),
            Action::SeekForward => tr!(
                "Forward by the short step (keyframes)",
                "Вперёд на короткий шаг (по ключевым кадрам)"
            ),
            Action::SeekBackLong => tr!(
                "Back by the long step (keyframes)",
                "Назад на длинный шаг (по ключевым кадрам)"
            ),
            Action::SeekForwardLong => tr!(
                "Forward by the long step (keyframes)",
                "Вперёд на длинный шаг (по ключевым кадрам)"
            ),
            Action::SeekBackExact => tr!("Back by 1 s, exact", "Назад на 1 с, точно"),
            Action::SeekForwardExact => tr!("Forward by 1 s, exact", "Вперёд на 1 с, точно"),
            Action::FrameBack => tr!("Previous frame", "Предыдущий кадр"),
            Action::FrameForward => tr!("Next frame", "Следующий кадр"),
            Action::Start => tr!("Start of the file", "Начало файла"),
            Action::End => tr!("End of the file", "Конец файла"),
            Action::Faster => tr!("Faster", "Быстрее"),
            Action::Slower => tr!("Slower", "Медленнее"),
            Action::NormalSpeed => tr!("Normal speed", "Обычная скорость"),
            Action::Position => tr!(
                "Position and chapter on the screen",
                "Позиция и глава на экране"
            ),
            Action::GoToTime => tr!("Go to time…", "Перейти ко времени…"),
            Action::AddBookmark => tr!("Add a bookmark here", "Добавить закладку здесь"),
            Action::PrevChapter => tr!(
                "Previous chapter (the start of this one after 5 s)",
                "Предыдущая глава (после 5 с — начало текущей)"
            ),
            Action::NextChapter => tr!("Next chapter", "Следующая глава"),
            Action::ChapterMarks => tr!(
                "Chapter marks on the seek bar",
                "Метки глав на полосе перемотки"
            ),
            Action::TimeDisplay => tr!(
                "Time: both, elapsed, remaining",
                "Время: оба, прошло, осталось"
            ),
            Action::Repeat => tr!(
                "Repeat: off, the file, the list",
                "Повтор: выключен, файл, плейлист"
            ),
            Action::LoopA => tr!("A-B repeat: start here", "Повтор A-B: начало здесь"),
            Action::LoopB => tr!("A-B repeat: end here", "Повтор A-B: конец здесь"),
            Action::LoopClear => tr!("A-B repeat off", "Выключить повтор A-B"),
            Action::LoopCycle => tr!(
                "A-B repeat: start, end, off",
                "Повтор A-B: начало, конец, выключить"
            ),
            Action::VolumeUp => tr!("Volume up", "Громче"),
            Action::VolumeDown => tr!("Volume down", "Тише"),
            Action::Mute => tr!("Mute", "Без звука"),
            Action::Tracks => tr!("Audio and subtitles window", "Окно звука и субтитров"),
            Action::NextAudio => tr!("Next audio track", "Следующая звуковая дорожка"),
            Action::NextSubs => tr!(
                "Next subtitles (then off)",
                "Следующие субтитры (затем выкл.)"
            ),
            Action::SubsOnOff => tr!("Subtitles off / on", "Субтитры выкл. / вкл."),
            Action::LoadSubs => tr!("Load subtitles from a file", "Загрузить субтитры из файла"),
            Action::SubsLater => tr!("Subtitles later by 100 ms", "Субтитры позже на 100 мс"),
            Action::SubsEarlier => tr!("Subtitles earlier by 100 ms", "Субтитры раньше на 100 мс"),
            Action::SubsInSync => tr!("Subtitles as they were", "Субтитры как было"),
            Action::SoundLater => tr!("Sound later by 100 ms", "Звук позже на 100 мс"),
            Action::SoundEarlier => tr!("Sound earlier by 100 ms", "Звук раньше на 100 мс"),
            Action::SoundInSync => tr!("Sound as it was", "Звук как было"),
            Action::ResetSubStyle => tr!("Reset the subtitles' look", "Сбросить вид субтитров"),
            Action::ResetEqualizer => tr!(
                "Reset the equalizer (flat, off)",
                "Сбросить эквалайзер (ровный, выкл.)"
            ),
            Action::EqualizerOnOff => tr!("Equalizer on / off", "Эквалайзер вкл. / выкл."),
            Action::Normalize => tr!("Normalization on / off", "Нормализация вкл. / выкл."),
            Action::VoiceBoost => tr!("Clearer speech on / off", "Усилить речь вкл. / выкл."),
            Action::Upmix => tr!("Stereo to 5.1 on / off", "Стерео в 5.1 вкл. / выкл."),
            Action::Fullscreen => tr!("Fullscreen", "Полный экран"),
            Action::Escape => tr!(
                "Close what is open, leave fullscreen, bring the interface back; else close the program",
                "Закрыть открытое, выйти из полноэкранного режима, вернуть интерфейс; иначе закрыть программу"
            ),
            Action::Scale50 => tr!("Window for 50% of the video", "Окно под 50% размера видео"),
            Action::Scale100 => tr!(
                "Window for 100% of the video",
                "Окно под 100% размера видео"
            ),
            Action::Scale150 => tr!(
                "Window for 150% of the video",
                "Окно под 150% размера видео"
            ),
            Action::Scale200 => tr!(
                "Window for 200% of the video",
                "Окно под 200% размера видео"
            ),
            Action::FitScreen => tr!("Window to the screen", "Окно по экрану"),
            Action::FitWindow => tr!("Window to the picture", "Окно по размеру картинки"),
            Action::Fill => tr!(
                "Fullscreen without black bars (crops) / with them",
                "Полный экран без чёрных полос (с обрезкой) / с полосами"
            ),
            Action::ResetPicture => tr!(
                "Reset the picture's settings (colour, zoom, filters, turns)",
                "Сбросить настройки изображения (цвет, масштаб, фильтры, повороты)"
            ),
            Action::Rotate => tr!("Turn 90° clockwise", "Повернуть на 90° по часовой"),
            Action::Aspect => tr!(
                "Aspect ratio: as in the file, 4:3, 16:9…",
                "Пропорции: как в файле, 4:3, 16:9…"
            ),
            Action::Deinterlace => tr!(
                "Deinterlacing: auto, on, off",
                "Деинтерлейсинг: авто, вкл., выкл."
            ),
            Action::ContrastDown => tr!("Contrast down", "Контраст меньше"),
            Action::ContrastUp => tr!("Contrast up", "Контраст больше"),
            Action::BrightnessDown => tr!("Brightness down", "Яркость меньше"),
            Action::BrightnessUp => tr!("Brightness up", "Яркость больше"),
            Action::GammaDown => tr!("Gamma down", "Гамма меньше"),
            Action::GammaUp => tr!("Gamma up", "Гамма больше"),
            Action::SaturationDown => tr!("Saturation down", "Насыщенность меньше"),
            Action::SaturationUp => tr!("Saturation up", "Насыщенность больше"),
            Action::ZoomIn => tr!("Zoom in", "Увеличить масштаб"),
            Action::ZoomOut => tr!("Zoom out", "Уменьшить масштаб"),
            Action::PanLeft => tr!("Move the picture left", "Сдвинуть изображение влево"),
            Action::PanRight => tr!("Move the picture right", "Сдвинуть изображение вправо"),
            Action::PanUp => tr!("Move the picture up", "Сдвинуть изображение вверх"),
            Action::PanDown => tr!("Move the picture down", "Сдвинуть изображение вниз"),
            Action::ZoomReset => tr!(
                "Zoom and position as they were",
                "Масштаб и положение как было"
            ),
            Action::Interface => tr!(
                "Only the picture / the interface again",
                "Только видео / вернуть интерфейс"
            ),
            Action::Interfaces => tr!(
                "Interface: full, without the title row, compact, only the picture",
                "Интерфейс: полный, без заголовка, компактный, только видео"
            ),
            Action::OnTop => tr!(
                "Over the other windows: never, always, while playing",
                "Поверх других окон: никогда, всегда, во время воспроизведения"
            ),
            Action::Minimize => tr!("Minimize the window", "Свернуть окно"),
            Action::Maximize => tr!(
                "Maximize / restore the window",
                "Развернуть / восстановить окно"
            ),
            Action::Prev => tr!("Previous file", "Предыдущий файл"),
            Action::Next => tr!("Next file", "Следующий файл"),
            Action::Open => tr!("Open files", "Открыть файлы"),
            Action::OpenUrl => tr!("Open an address (a stream)", "Открыть адрес (поток)"),
            Action::Playlist => tr!("Show or hide the playlist", "Показать или скрыть плейлист"),
            Action::PlaylistWindow => tr!(
                "Playlist in a window of its own / in the player",
                "Плейлист в отдельном окне / в окне плеера"
            ),
            Action::Delete => tr!(
                "Remove the selected playlist entry",
                "Убрать выбранную запись из плейлиста"
            ),
            Action::PlaylistAdd => tr!("Add files to the playlist", "Добавить файлы в плейлист"),
            Action::PlaylistSave => tr!("Save the playlist (M3U8)", "Сохранить плейлист (M3U8)"),
            Action::PlaylistShuffle => tr!("Shuffle the playlist", "Перемешать плейлист"),
            Action::PlaylistSort => tr!("Sort the playlist", "Сортировать плейлист"),
            Action::PlaylistClear => tr!("Clear the playlist", "Очистить плейлист"),
            Action::ShowInFolder => tr!("Show the file in its folder", "Показать файл в папке"),
            Action::Info => tr!("File information", "Сведения о файле"),
            Action::VideoPanel => tr!(
                "Video, sound and subtitles panel",
                "Панель видео, звука и субтитров"
            ),
            Action::Equalizer => tr!("Equalizer", "Эквалайзер"),
            Action::SaveFrame => tr!("Save the frame", "Сохранить кадр"),
            Action::ShowFrames => tr!(
                "Open the folder of saved frames",
                "Открыть папку сохранённых кадров"
            ),
            Action::SaveCover => tr!("Save the cover…", "Сохранить обложку…"),
            Action::Settings => tr!("Settings", "Настройки"),
            Action::About => tr!("About", "О программе"),
            Action::Close => tr!("Close the program", "Закрыть программу"),
        }
    }

    /// Whether a held key repeats it: seeking, frame steps, the volume, the
    /// delays, the colour, the zoom and the pan.
    pub fn repeats(self) -> bool {
        matches!(
            self.cmd(),
            Cmd::SeekShort(_)
                | Cmd::SeekLong(_)
                | Cmd::SeekExact(_)
                | Cmd::Frame { .. }
                | Cmd::Volume(_)
                | Cmd::AudioDelay(_)
                | Cmd::SubDelay(_)
                | Cmd::Color(..)
                | Cmd::Zoom(_)
                | Cmd::Pan(..)
        )
    }

    fn index(self) -> usize {
        Action::ALL
            .iter()
            .position(|&a| a == self)
            .expect("every action is in ALL")
    }

    fn from_id(id: &str) -> Option<Action> {
        Action::ALL.iter().copied().find(|a| a.id() == id)
    }
}

/// The chords of every action, in the order of `Action::ALL`.
#[derive(Clone, Debug, PartialEq)]
pub struct Keymap {
    chords: Vec<Vec<Chord>>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self {
            chords: Action::ALL.iter().map(|a| a.defaults().to_vec()).collect(),
        }
    }
}

impl Keymap {
    pub fn chords(&self, action: Action) -> &[Chord] {
        &self.chords[action.index()]
    }

    /// The action of `chord`, if any.
    pub fn action(&self, chord: Chord) -> Option<Action> {
        Action::ALL
            .iter()
            .copied()
            .find(|&a| self.chords(a).contains(&chord))
    }

    /// Give `action` the chord, in place of its chord `slot` or as one more
    /// (`None`); the action that had the chord loses it and is returned.
    pub fn assign(&mut self, action: Action, slot: Option<usize>, chord: Chord) -> Option<Action> {
        let mine = &self.chords[action.index()];
        if mine.contains(&chord) {
            return None;
        }
        let taken = self.action(chord);
        if let Some(other) = taken {
            self.chords[other.index()].retain(|&c| c != chord);
        }
        let mine = &mut self.chords[action.index()];
        match slot {
            Some(i) if i < mine.len() => mine[i] = chord,
            _ => mine.push(chord),
        }
        taken
    }

    pub fn remove(&mut self, action: Action, slot: usize) {
        let mine = &mut self.chords[action.index()];
        if slot < mine.len() {
            mine.remove(slot);
        }
    }

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The actions whose keys differ from the defaults, a line each:
    /// `id<TAB>chord chord` (nothing after the tab for no keys).
    pub fn to_setting(&self) -> String {
        Action::ALL
            .iter()
            .filter(|a| self.chords(**a) != a.defaults())
            .map(|a| {
                let chords: Vec<String> = self.chords(*a).iter().map(Chord::name).collect();
                format!("{}\t{}", a.id(), chords.join(" "))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Read what `to_setting` wrote; unknown actions and chords are left
    /// out, a chord given to two actions stays with the first.
    pub fn from_setting(text: &str) -> Self {
        let mut map = Self::default();
        let mut changed = Vec::new();
        for line in text.lines() {
            let Some((id, chords)) = line.split_once('\t') else {
                continue;
            };
            let Some(action) = Action::from_id(id.trim()) else {
                continue;
            };
            if changed.contains(&action) {
                continue;
            }
            let mut mine: Vec<Chord> = Vec::new();
            for chord in chords.split_whitespace().filter_map(Chord::from_name) {
                let taken = changed.iter().any(|a| map.chords(*a).contains(&chord));
                if !taken && !mine.contains(&chord) {
                    mine.push(chord);
                }
            }
            map.chords[action.index()] = mine;
            changed.push(action);
        }
        // A default chord of an action not named may have been given to
        // one that is: the named one keeps it.
        for action in Action::ALL {
            if changed.contains(action) {
                continue;
            }
            let taken: Vec<Chord> = changed
                .iter()
                .flat_map(|a| map.chords(*a).to_vec())
                .collect();
            map.chords[action.index()].retain(|c| !taken.contains(c));
        }
        map
    }
}

/// The keys in use, for the labels of the menus and tooltips.
static CURRENT: LazyLock<RwLock<Keymap>> = LazyLock::new(|| RwLock::new(Keymap::default()));

/// Make `map` the one the labels show.
pub fn publish(map: &Keymap) {
    if let Ok(mut current) = CURRENT.write() {
        current.clone_from(map);
    }
}

/// The label of the first key of the action that does `cmd`, or nothing.
pub fn shortcut(cmd: Cmd) -> String {
    let Ok(map) = CURRENT.read() else {
        return String::new();
    };
    Action::ALL
        .iter()
        .find(|a| a.cmd() == cmd)
        .and_then(|a| map.chords(*a).first())
        .map(Chord::label)
        .unwrap_or_default()
}

/// `text` with the key of `cmd` in brackets after it, if it has one.
pub fn with_key(text: &str, cmd: Cmd) -> String {
    let key = shortcut(cmd);
    if key.is_empty() {
        text.to_owned()
    } else {
        format!("{text} ({key})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_and_cut_keep_their_modifiers() {
        let name = |event, m| Chord::of_event(&event, m).map(|(c, _)| c.name());
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(
            name(egui::Event::Copy, Modifiers::CTRL).as_deref(),
            Some("Ctrl+C")
        );
        assert_eq!(
            name(egui::Event::Copy, ctrl_shift).as_deref(),
            Some("Ctrl+Shift+C")
        );
        assert_eq!(
            name(egui::Event::Cut, Modifiers::CTRL).as_deref(),
            Some("Ctrl+X")
        );
        assert_eq!(
            name(egui::Event::Cut, Modifiers::SHIFT).as_deref(),
            Some("Shift+Delete")
        );
    }

    #[test]
    fn ids_and_defaults_are_unique() {
        let mut ids = std::collections::HashSet::new();
        let mut chords = std::collections::HashSet::new();
        for a in Action::ALL {
            assert!(ids.insert(a.id()), "{} twice", a.id());
            for c in a.defaults() {
                assert!(chords.insert(*c), "{} twice", c.name());
            }
        }
    }

    #[test]
    fn names_round_trip() {
        for key in Key::ALL {
            let chord = Chord::new(Press::Key(*key), Modifiers::NONE);
            assert_eq!(Chord::from_name(&chord.name()), Some(chord), "{key:?}");
        }
        for name in [
            "Ctrl+Shift+Left",
            "Alt+MouseBack",
            "MouseForward",
            "Ctrl+Comma",
        ] {
            assert_eq!(Chord::from_name(name).unwrap().name(), name);
        }
        assert_eq!(Chord::from_name("Ctrl+"), None);
        assert_eq!(Chord::from_name("Ctrl"), None);
        assert_eq!(Chord::from_name("Ctrl+Nothing"), None);
    }

    #[test]
    fn modifiers_alone_are_no_chord() {
        assert_eq!(Chord::of_key(Key::ShiftLeft, Modifiers::SHIFT), None);
        assert_eq!(
            Chord::of_key(Key::A, Modifiers::COMMAND),
            Some(Chord::ctrl(Key::A))
        );
    }

    #[test]
    fn assigning_takes_the_chord_from_another() {
        let mut map = Keymap::default();
        let space = Chord::key(Key::Space);
        assert_eq!(map.assign(Action::Stop, None, space), Some(Action::Pause));
        assert_eq!(map.action(space), Some(Action::Stop));
        assert!(map.chords(Action::Pause).is_empty());
        // In place of the second key.
        let g = Chord::key(Key::G);
        assert_eq!(map.assign(Action::NextSubs, Some(1), g), None);
        assert_eq!(map.chords(Action::NextSubs), [Chord::key(Key::S), g]);
        map.remove(Action::NextSubs, 0);
        assert_eq!(map.chords(Action::NextSubs), [g]);
    }

    #[test]
    fn only_changes_are_kept() {
        assert_eq!(Keymap::default().to_setting(), "");
        let mut map = Keymap::default();
        map.assign(Action::Stop, None, Chord::key(Key::Space));
        map.remove(Action::VideoPanel, 1);
        let text = map.to_setting();
        assert_eq!(text, "pause\t\nstop\tSpace\nvideo_panel\tU");
        assert_eq!(Keymap::from_setting(&text), map);
        assert!(Keymap::from_setting("").is_default());
        assert!(Keymap::from_setting("nothing\tSpace\npause\tSpace Bad+Key").is_default());
        let map = Keymap::from_setting("stop\tG\nminimize\tG H");
        assert_eq!(map.chords(Action::Stop), [Chord::key(Key::G)]);
        assert_eq!(map.chords(Action::Minimize), [Chord::key(Key::H)]);
    }

    #[test]
    fn a_named_action_keeps_a_default_chord() {
        // Written by a version that had no `pause` default of Space.
        let map = Keymap::from_setting("stop\tSpace");
        assert_eq!(map.action(Chord::key(Key::Space)), Some(Action::Stop));
        assert!(map.chords(Action::Pause).is_empty());
    }

    #[test]
    fn shortcut_labels() {
        publish(&Keymap::default());
        assert_eq!(shortcut(Cmd::Open), "Ctrl+O");
        assert_eq!(shortcut(Cmd::Stop), "");
        assert_eq!(shortcut(Cmd::SeekLong(-1.0)), "Ctrl+←");
        assert_eq!(with_key("Playlist", Cmd::TogglePlaylist), "Playlist (L)");
        assert_eq!(with_key("Stop", Cmd::Stop), "Stop");
    }
}
