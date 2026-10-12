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
    ToggleFullscreen,
    LeaveFullscreen,
    Next,
    Prev,
    Open,
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
    /// Save the file's cover as a picture file.
    SaveCover,
    /// Show the file in Explorer.
    ShowInFolder,
    /// The time in the control bar: both, elapsed, remaining (a click on it).
    CycleTime,
    /// The File Associations dialog (the context menu).
    Associations,
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
        Key::Escape => Cmd::LeaveFullscreen,
        Key::O if ctrl => Cmd::Open,
        Key::L if !ctrl => Cmd::TogglePlaylist,
        Key::F1 => Cmd::About,
        Key::F5 => Cmd::Settings,
        Key::Delete => Cmd::Delete,
        Key::T if !ctrl => Cmd::ToggleTracks,
        Key::I if !ctrl => Cmd::ToggleInfo,
        Key::A if !ctrl => Cmd::CycleAudio,
        // S as in subtitles; J as in mpv.
        Key::S | Key::J if !ctrl => Cmd::CycleSubs,
        Key::V if !ctrl => Cmd::ToggleSubs,
        _ => return None,
    };
    let repeats = matches!(
        cmd,
        Cmd::SeekShort(_)
            | Cmd::SeekLong(_)
            | Cmd::SeekExact(_)
            | Cmd::Frame { .. }
            | Cmd::Volume(_)
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
    fn other_keys() {
        assert_eq!(command(Key::O, Modifiers::COMMAND, false), Some(Cmd::Open));
        assert_eq!(command(Key::O, Modifiers::NONE, false), None);
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
