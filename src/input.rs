//! Keys of the player. `Event::Key::key` is the logical key, or the
//! physical one for a letter of a non-Latin layout (egui-winit), so `M`
//! and `,` `.` work in the Russian layout too.

use egui::{Event, Key, Modifiers};

/// What a key, a button or a menu asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    TogglePause,
    Stop,
    /// Seek by the short (Left, Right) or long (Ctrl) step on a keyframe,
    /// `1` or `-1` times.
    SeekShort(f64),
    SeekLong(f64),
    /// Seek by seconds to the exact frame (Shift+Left, Shift+Right).
    SeekExact(f64),
    Frame {
        forward: bool,
    },
    Start,
    End,
    Volume(f64),
    ToggleMute,
    /// Move the sound by this many steps of `audio::DELAY_STEP` against
    /// the picture (`Ctrl+=` later, `Ctrl+-` earlier); 0 puts it back
    /// (`Ctrl+0`).
    AudioDelay(f64),
    /// Move the subtitles by this many steps of `audio::DELAY_STEP`
    /// (`Shift+.` later, `Shift+,` earlier); 0 puts them back (`Shift+0`).
    SubDelay(f64),
    /// The playback speed by this many steps of `format::SPEEDS` (`C`
    /// faster, `X` slower), 0 for normal (`Z`), as in GOM Player.
    Speed(i32),
    /// The playback speed itself (the context menu).
    SetSpeed(f64),
    ToggleFullscreen,
    /// Close the information window, else leave fullscreen, else close
    /// the program.
    Escape,
    Next,
    Prev,
    Open,
    /// Ask for an address to play (`Ctrl+U`).
    OpenUrl,
    TogglePlaylist,
    About,
    Settings,
    /// Delete the selected playlist entry.
    Delete,
    // Commands of the mouse only.
    /// Move to `seconds` (the seek bar).
    SeekTo {
        seconds: f64,
        exact: bool,
    },
    SetVolume(f64),
    PlayIndex(usize),
    PlaylistAdd,
    PlaylistRemove(usize),
    PlaylistSort,
    PlaylistClear,
    /// Write the playlist to an M3U8 file.
    PlaylistSave,
    /// Put the playlist in a random order.
    PlaylistShuffle,
    /// Repeat off, the file, the list (`R`).
    CycleRepeat,
    /// Over the other windows: never, always, while playing (`Ctrl+T`).
    CycleOnTop,
    SetOnTop(crate::settings::OnTop),
    SetRepeat(crate::playlist::Repeat),
    /// Open or close the tracks window (`T`).
    ToggleTracks,
    /// The next audio track (`A`), the next subtitles or none (`S`, `J`),
    /// subtitles off and on again (`V`).
    CycleAudio,
    CycleSubs,
    ToggleSubs,
    SelectAudio(i64),
    SelectSub(Option<i64>),
    /// Subtitles from a file of their own.
    LoadSubtitles,
    /// Go to the start of chapter `n` (from 0).
    Chapter(usize),
    /// Show or hide the chapter marks on the seek bar.
    ToggleChapterMarks,
    /// Open or close the file's information window (`I`).
    ToggleInfo,
    /// Open or close the Video, sound and subtitles panel (`U`, next to the
    /// information's `I`; `F7` as in GOM Player).
    ToggleVideoPanel,
    /// Open the panel on its Sound tab (the context menu).
    ShowEqualizer,
    /// Save the frame on screen into Pictures\MetalMPV (`Ctrl+E`).
    SaveFrame,
    /// Open the folder of the saved frames.
    ShowFrames,
    /// Save the file's cover as a picture file.
    SaveCover,
    /// Play recent file `n` of the start screen.
    OpenRecent(usize),
    ClearRecent,
    /// Show the file in Explorer.
    ShowInFolder,
    /// The time in the control bar: both, elapsed, remaining (a click on it).
    CycleTime,
    /// The position, the duration and the chapter on the OSD (`O`).
    ShowPosition,
    /// Size the window for the picture at this part of the video's size
    /// (`1` to `4`: 0.5, 1, 1.5, 2).
    WindowScale(f64),
    /// The largest window the work area holds, the picture without black
    /// bars (`5`).
    FitScreen,
    /// Size the window to the picture shown, without the black bars
    /// (`` ` ``, `Ё` in the Russian layout).
    FitWindow,
    /// Fullscreen with the picture filling the screen, cropped, or the
    /// whole picture again, still in fullscreen (`6`).
    ToggleFill,
    /// Only the picture in the window, or the title row, the control bar
    /// and the playlist again (`7`, the context menu).
    ToggleInterface,
    /// Force the picture's shape, or the file's again (the context menu).
    SetAspect(crate::adjust::Aspect),
    Minimize,
    ToggleMaximize,
    Close,
}

/// The step of the volume keys and of a wheel notch, in percent.
pub const VOLUME_STEP: f64 = 5.0;

/// The command of `key` with `m` held; `repeat` is a key held down, which
/// repeats seeking, frame steps and the volume only.
pub fn command(key: Key, m: Modifiers, repeat: bool) -> Option<Cmd> {
    let ctrl = m.command;
    let cmd = match key {
        Key::Space if !ctrl => Cmd::TogglePause,
        Key::ArrowLeft if m.shift => Cmd::SeekExact(-1.0),
        Key::ArrowRight if m.shift => Cmd::SeekExact(1.0),
        Key::ArrowLeft if ctrl => Cmd::SeekLong(-1.0),
        Key::ArrowRight if ctrl => Cmd::SeekLong(1.0),
        Key::ArrowLeft => Cmd::SeekShort(-1.0),
        Key::ArrowRight => Cmd::SeekShort(1.0),
        Key::Comma if ctrl => Cmd::Settings,
        // `<` and `>`: egui has no such keys and passes the physical ones.
        Key::Comma if m.shift => Cmd::SubDelay(-1.0),
        Key::Period if m.shift => Cmd::SubDelay(1.0),
        Key::Num0 if m.shift && !ctrl => Cmd::SubDelay(0.0),
        // As in mpv: Ctrl with + and -.
        Key::Plus | Key::Equals if ctrl => Cmd::AudioDelay(1.0),
        Key::Minus if ctrl => Cmd::AudioDelay(-1.0),
        Key::Num0 if ctrl => Cmd::AudioDelay(0.0),
        Key::Comma | Key::D => Cmd::Frame { forward: false },
        Key::Period | Key::F => Cmd::Frame { forward: true },
        Key::ArrowUp => Cmd::Volume(VOLUME_STEP),
        Key::ArrowDown => Cmd::Volume(-VOLUME_STEP),
        Key::M if !ctrl => Cmd::ToggleMute,
        Key::Home => Cmd::Start,
        Key::End => Cmd::End,
        Key::PageUp => Cmd::Prev,
        Key::PageDown => Cmd::Next,
        Key::Enter | Key::F11 if !ctrl => Cmd::ToggleFullscreen,
        Key::Escape => Cmd::Escape,
        Key::O if ctrl => Cmd::Open,
        Key::U if ctrl => Cmd::OpenUrl,
        // O as in mpv: where playback is.
        Key::O => Cmd::ShowPosition,
        Key::L if !ctrl => Cmd::TogglePlaylist,
        Key::F1 => Cmd::About,
        Key::F5 => Cmd::Settings,
        Key::U | Key::F7 => Cmd::ToggleVideoPanel,
        Key::E if ctrl => Cmd::SaveFrame,
        Key::Delete => Cmd::Delete,
        Key::T if ctrl => Cmd::CycleOnTop,
        Key::T => Cmd::ToggleTracks,
        Key::I if !ctrl => Cmd::ToggleInfo,
        Key::A if !ctrl => Cmd::CycleAudio,
        // S as in subtitles; J as in mpv.
        Key::S | Key::J if !ctrl => Cmd::CycleSubs,
        Key::V if !ctrl => Cmd::ToggleSubs,
        Key::R if !ctrl && !m.alt => Cmd::CycleRepeat,
        Key::C if !ctrl && !m.alt => Cmd::Speed(1),
        Key::X if !ctrl && !m.alt => Cmd::Speed(-1),
        Key::Z if !ctrl && !m.alt => Cmd::Speed(0),
        Key::Num1 if !ctrl && !m.alt => Cmd::WindowScale(0.5),
        Key::Num2 if !ctrl && !m.alt => Cmd::WindowScale(1.0),
        Key::Num3 if !ctrl && !m.alt => Cmd::WindowScale(1.5),
        Key::Num4 if !ctrl && !m.alt => Cmd::WindowScale(2.0),
        Key::Num5 if !ctrl && !m.alt => Cmd::FitScreen,
        // The key left of 1: `Ё` in the Russian layout.
        Key::Backtick if !ctrl && !m.alt => Cmd::FitWindow,
        Key::Num6 if !ctrl && !m.alt => Cmd::ToggleFill,
        Key::Num7 if !ctrl && !m.alt => Cmd::ToggleInterface,
        _ => return None,
    };
    let repeats = matches!(
        cmd,
        Cmd::SeekShort(_)
            | Cmd::SeekLong(_)
            | Cmd::SeekExact(_)
            | Cmd::Frame { .. }
            | Cmd::Volume(_)
            | Cmd::AudioDelay(_)
            | Cmd::SubDelay(_)
    );
    (!repeat || repeats).then_some(cmd)
}

/// The commands of this frame's keys, in order. Consumed keys are not
/// seen by the widgets; call it when no text field has the focus.
pub fn keys(ctx: &egui::Context) -> Vec<Cmd> {
    ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                Event::Key {
                    key,
                    pressed: true,
                    repeat,
                    modifiers,
                    ..
                } => command(*key, *modifiers, *repeat),
                _ => None,
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_seek() {
        let none = Modifiers::NONE;
        assert_eq!(
            command(Key::ArrowRight, none, false),
            Some(Cmd::SeekShort(1.0))
        );
        assert_eq!(
            command(Key::ArrowLeft, Modifiers::COMMAND, false),
            Some(Cmd::SeekLong(-1.0))
        );
        assert_eq!(
            command(Key::ArrowLeft, Modifiers::SHIFT, true),
            Some(Cmd::SeekExact(-1.0))
        );
        assert_eq!(
            command(Key::ArrowLeft, Modifiers::SHIFT | Modifiers::COMMAND, false),
            Some(Cmd::SeekExact(-1.0))
        );
    }

    #[test]
    fn held_keys_repeat_only_motion() {
        assert_eq!(command(Key::Space, Modifiers::NONE, true), None);
        assert_eq!(command(Key::Enter, Modifiers::NONE, true), None);
        assert_eq!(
            command(Key::ArrowUp, Modifiers::NONE, true),
            Some(Cmd::Volume(VOLUME_STEP))
        );
        assert_eq!(
            command(Key::Period, Modifiers::NONE, true),
            Some(Cmd::Frame { forward: true })
        );
    }

    #[test]
    fn shift_comma_period_move_the_subtitles() {
        let shift = Modifiers::SHIFT;
        assert_eq!(command(Key::Comma, shift, false), Some(Cmd::SubDelay(-1.0)));
        assert_eq!(command(Key::Period, shift, true), Some(Cmd::SubDelay(1.0)));
        assert_eq!(command(Key::Num0, shift, false), Some(Cmd::SubDelay(0.0)));
        assert_eq!(
            command(Key::Comma, Modifiers::NONE, false),
            Some(Cmd::Frame { forward: false })
        );
    }

    #[test]
    fn ctrl_t_on_top_t_tracks() {
        assert_eq!(
            command(Key::T, Modifiers::COMMAND, false),
            Some(Cmd::CycleOnTop)
        );
        assert_eq!(
            command(Key::T, Modifiers::NONE, false),
            Some(Cmd::ToggleTracks)
        );
    }

    #[test]
    fn c_x_z_set_the_speed() {
        let none = Modifiers::NONE;
        assert_eq!(command(Key::C, none, false), Some(Cmd::Speed(1)));
        assert_eq!(command(Key::X, none, false), Some(Cmd::Speed(-1)));
        assert_eq!(command(Key::Z, none, false), Some(Cmd::Speed(0)));
        assert_eq!(command(Key::C, Modifiers::COMMAND, false), None);
    }

    #[test]
    fn ctrl_plus_minus_move_the_sound() {
        let ctrl = Modifiers::COMMAND;
        assert_eq!(
            command(Key::Equals, ctrl, false),
            Some(Cmd::AudioDelay(1.0))
        );
        assert_eq!(command(Key::Plus, ctrl, true), Some(Cmd::AudioDelay(1.0)));
        assert_eq!(
            command(Key::Minus, ctrl, false),
            Some(Cmd::AudioDelay(-1.0))
        );
        assert_eq!(command(Key::Num0, ctrl, false), Some(Cmd::AudioDelay(0.0)));
        assert_eq!(command(Key::Minus, Modifiers::NONE, false), None);
    }

    #[test]
    fn digits_size_the_picture() {
        let none = Modifiers::NONE;
        assert_eq!(command(Key::Num1, none, false), Some(Cmd::WindowScale(0.5)));
        assert_eq!(command(Key::Num2, none, false), Some(Cmd::WindowScale(1.0)));
        assert_eq!(command(Key::Num3, none, false), Some(Cmd::WindowScale(1.5)));
        assert_eq!(command(Key::Num4, none, false), Some(Cmd::WindowScale(2.0)));
        assert_eq!(command(Key::Num5, none, false), Some(Cmd::FitScreen));
        assert_eq!(command(Key::Backtick, none, false), Some(Cmd::FitWindow));
        assert_eq!(command(Key::Num6, none, false), Some(Cmd::ToggleFill));
        assert_eq!(command(Key::Num7, none, false), Some(Cmd::ToggleInterface));
        assert_eq!(command(Key::Num4, none, true), None);
        assert_eq!(command(Key::Num1, Modifiers::COMMAND, false), None);
        assert_eq!(command(Key::Num1, Modifiers::ALT, false), None);
    }

    #[test]
    fn other_keys() {
        assert_eq!(command(Key::O, Modifiers::COMMAND, false), Some(Cmd::Open));
        assert_eq!(
            command(Key::F7, Modifiers::NONE, false),
            Some(Cmd::ToggleVideoPanel)
        );
        assert_eq!(
            command(Key::U, Modifiers::NONE, false),
            Some(Cmd::ToggleVideoPanel)
        );
        assert_eq!(
            command(Key::U, Modifiers::COMMAND, false),
            Some(Cmd::OpenUrl)
        );
        assert_eq!(
            command(Key::E, Modifiers::COMMAND, false),
            Some(Cmd::SaveFrame)
        );
        assert_eq!(command(Key::E, Modifiers::NONE, false), None);
        assert_eq!(
            command(Key::O, Modifiers::NONE, false),
            Some(Cmd::ShowPosition)
        );
        assert_eq!(
            command(Key::F11, Modifiers::NONE, false),
            Some(Cmd::ToggleFullscreen)
        );
        assert_eq!(
            command(Key::Comma, Modifiers::COMMAND, false),
            Some(Cmd::Settings)
        );
        assert_eq!(
            command(Key::A, Modifiers::NONE, false),
            Some(Cmd::CycleAudio)
        );
        assert_eq!(command(Key::J, Modifiers::NONE, true), None);
        assert_eq!(
            command(Key::S, Modifiers::NONE, false),
            Some(Cmd::CycleSubs)
        );
    }
}
