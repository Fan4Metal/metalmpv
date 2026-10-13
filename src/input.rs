//! The commands of the player and the keys that give them (`keymap.rs`).
//! `Event::Key::key` is the logical key, or the physical one for a letter
//! of a non-Latin layout (egui-winit), so `M` and `,` `.` work in the
//! Russian layout too.

use crate::keymap::{Chord, Keymap};

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
    /// The playlist in a window of its own, or back in the player's.
    TogglePlaylistWindow,
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

/// The command of `chord` in `map`; `repeat` is a key held down, which
/// repeats seeking, frame steps, the volume and the delays only.
pub fn command(map: &Keymap, chord: Chord, repeat: bool) -> Option<Cmd> {
    let action = map.action(chord)?;
    (!repeat || action.repeats()).then(|| action.cmd())
}

/// The commands of this frame's keys and of the mouse's side buttons, in
/// order. Consumed keys are not seen by the widgets; call it when no text
/// field has the focus.
pub fn keys(ctx: &egui::Context, map: &Keymap) -> Vec<Cmd> {
    ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| {
                let (chord, repeat) = Chord::of_event(e)?;
                command(map, chord, repeat)
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Key, Modifiers, PointerButton};

    /// The default command of `key` with `m`.
    fn default_command(key: Key, m: Modifiers, repeat: bool) -> Option<Cmd> {
        command(&Keymap::default(), Chord::of_key(key, m)?, repeat)
    }

    #[test]
    fn side_buttons_seek() {
        let map = Keymap::default();
        let button = |b, m| command(&map, Chord::of_button(b, m).unwrap(), false);
        assert_eq!(
            button(PointerButton::Extra1, Modifiers::NONE),
            Some(Cmd::SeekShort(-1.0))
        );
        assert_eq!(
            button(PointerButton::Extra2, Modifiers::COMMAND),
            Some(Cmd::SeekLong(1.0))
        );
        assert_eq!(
            button(PointerButton::Extra1, Modifiers::SHIFT),
            Some(Cmd::SeekExact(-1.0))
        );
        assert_eq!(
            Chord::of_button(PointerButton::Primary, Modifiers::NONE),
            None
        );
    }

    #[test]
    fn arrows_seek() {
        let none = Modifiers::NONE;
        assert_eq!(
            default_command(Key::ArrowRight, none, false),
            Some(Cmd::SeekShort(1.0))
        );
        assert_eq!(
            default_command(Key::ArrowLeft, Modifiers::COMMAND, false),
            Some(Cmd::SeekLong(-1.0))
        );
        assert_eq!(
            default_command(Key::ArrowLeft, Modifiers::SHIFT, true),
            Some(Cmd::SeekExact(-1.0))
        );
    }

    #[test]
    fn held_keys_repeat_only_motion() {
        assert_eq!(default_command(Key::Space, Modifiers::NONE, true), None);
        assert_eq!(default_command(Key::Enter, Modifiers::NONE, true), None);
        assert_eq!(
            default_command(Key::ArrowUp, Modifiers::NONE, true),
            Some(Cmd::Volume(VOLUME_STEP))
        );
        assert_eq!(
            default_command(Key::Period, Modifiers::NONE, true),
            Some(Cmd::Frame { forward: true })
        );
    }

    #[test]
    fn shift_comma_period_move_the_subtitles() {
        let shift = Modifiers::SHIFT;
        assert_eq!(
            default_command(Key::Comma, shift, false),
            Some(Cmd::SubDelay(-1.0))
        );
        assert_eq!(
            default_command(Key::Period, shift, true),
            Some(Cmd::SubDelay(1.0))
        );
        assert_eq!(
            default_command(Key::Num0, shift, false),
            Some(Cmd::SubDelay(0.0))
        );
        assert_eq!(
            default_command(Key::Comma, Modifiers::NONE, false),
            Some(Cmd::Frame { forward: false })
        );
    }

    #[test]
    fn ctrl_t_on_top_t_tracks() {
        assert_eq!(
            default_command(Key::T, Modifiers::COMMAND, false),
            Some(Cmd::CycleOnTop)
        );
        assert_eq!(
            default_command(Key::T, Modifiers::NONE, false),
            Some(Cmd::ToggleTracks)
        );
    }

    #[test]
    fn c_x_z_set_the_speed() {
        let none = Modifiers::NONE;
        assert_eq!(default_command(Key::C, none, false), Some(Cmd::Speed(1)));
        assert_eq!(default_command(Key::X, none, false), Some(Cmd::Speed(-1)));
        assert_eq!(default_command(Key::Z, none, false), Some(Cmd::Speed(0)));
        assert_eq!(default_command(Key::C, Modifiers::COMMAND, false), None);
    }

    #[test]
    fn ctrl_plus_minus_move_the_sound() {
        let ctrl = Modifiers::COMMAND;
        assert_eq!(
            default_command(Key::Equals, ctrl, false),
            Some(Cmd::AudioDelay(1.0))
        );
        assert_eq!(
            default_command(Key::Plus, ctrl, true),
            Some(Cmd::AudioDelay(1.0))
        );
        assert_eq!(
            default_command(Key::Minus, ctrl, false),
            Some(Cmd::AudioDelay(-1.0))
        );
        assert_eq!(
            default_command(Key::Num0, ctrl, false),
            Some(Cmd::AudioDelay(0.0))
        );
        assert_eq!(default_command(Key::Minus, Modifiers::NONE, false), None);
    }

    #[test]
    fn digits_size_the_picture() {
        let none = Modifiers::NONE;
        let scale = |key| default_command(key, none, false);
        assert_eq!(scale(Key::Num1), Some(Cmd::WindowScale(0.5)));
        assert_eq!(scale(Key::Num2), Some(Cmd::WindowScale(1.0)));
        assert_eq!(scale(Key::Num3), Some(Cmd::WindowScale(1.5)));
        assert_eq!(scale(Key::Num4), Some(Cmd::WindowScale(2.0)));
        assert_eq!(scale(Key::Num5), Some(Cmd::FitScreen));
        assert_eq!(scale(Key::Backtick), Some(Cmd::FitWindow));
        assert_eq!(scale(Key::Num6), Some(Cmd::ToggleFill));
        assert_eq!(scale(Key::Num7), Some(Cmd::ToggleInterface));
        assert_eq!(default_command(Key::Num4, none, true), None);
        assert_eq!(default_command(Key::Num1, Modifiers::COMMAND, false), None);
        assert_eq!(default_command(Key::Num1, Modifiers::ALT, false), None);
    }

    #[test]
    fn other_keys() {
        let none = Modifiers::NONE;
        let ctrl = Modifiers::COMMAND;
        assert_eq!(default_command(Key::O, ctrl, false), Some(Cmd::Open));
        assert_eq!(
            default_command(Key::F7, none, false),
            Some(Cmd::ToggleVideoPanel)
        );
        assert_eq!(
            default_command(Key::U, none, false),
            Some(Cmd::ToggleVideoPanel)
        );
        assert_eq!(default_command(Key::U, ctrl, false), Some(Cmd::OpenUrl));
        assert_eq!(default_command(Key::E, ctrl, false), Some(Cmd::SaveFrame));
        assert_eq!(default_command(Key::E, none, false), None);
        assert_eq!(
            default_command(Key::O, none, false),
            Some(Cmd::ShowPosition)
        );
        assert_eq!(
            default_command(Key::F11, none, false),
            Some(Cmd::ToggleFullscreen)
        );
        assert_eq!(
            default_command(Key::Enter, Modifiers::ALT, false),
            Some(Cmd::ToggleFullscreen)
        );
        assert_eq!(
            default_command(Key::Comma, ctrl, false),
            Some(Cmd::Settings)
        );
        assert_eq!(default_command(Key::A, none, false), Some(Cmd::CycleAudio));
        assert_eq!(default_command(Key::J, none, true), None);
        assert_eq!(default_command(Key::S, none, false), Some(Cmd::CycleSubs));
    }

    #[test]
    fn changed_keys() {
        let mut map = Keymap::default();
        let g = Chord::of_key(Key::G, Modifiers::NONE).unwrap();
        map.assign(crate::keymap::Action::Stop, None, g);
        assert_eq!(command(&map, g, false), Some(Cmd::Stop));
        let space = Chord::of_key(Key::Space, Modifiers::NONE).unwrap();
        map.remove(crate::keymap::Action::Pause, 0);
        assert_eq!(command(&map, space, false), None);
    }
}
