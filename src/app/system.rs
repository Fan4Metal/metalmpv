//! Windows around the player: hooked messages and media keys, the taskbar
//! button, the media controls, on top, sleep and the audio device.

use super::*;

impl App {
    /// The media keys of a keyboard, a headset or a remote (the window's
    /// messages, `win::hook_window`), as the player's commands.
    pub(super) fn take_hooked(&mut self) {
        let paused = self
            .player
            .as_ref()
            .is_none_or(|p| p.state.paused || p.state.idle);
        // Play with nothing to play would open the file dialog, perhaps
        // over a minimised window: a headset's button does nothing then.
        let nothing = self.state_idle() && self.playlist.is_empty();
        let play = |cmd: Cmd| (!(nothing && cmd == Cmd::TogglePause)).then_some(cmd);
        for hooked in win::take_hooked() {
            let (key, via) = match hooked {
                win::Hooked::Media(key, via) => (key, via),
                win::Hooked::TaskbarButton => {
                    if let Some(taskbar) = &mut self.taskbar {
                        taskbar.button_created();
                    }
                    continue;
                }
                win::Hooked::EndSession => {
                    self.save_now = Some("the session is ending");
                    continue;
                }
                win::Hooked::Suspend => {
                    self.save_now = Some("going to sleep");
                    continue;
                }
                win::Hooked::Close => {
                    log::info!("the Restart Manager asks to close");
                    self.pending.push(Cmd::Close);
                    continue;
                }
                win::Hooked::Thumb(id) => {
                    self.pending.extend(match id {
                        crate::taskbar::PREVIOUS => Some(Cmd::Prev),
                        crate::taskbar::PLAY_PAUSE => play(Cmd::TogglePause),
                        crate::taskbar::NEXT => Some(Cmd::Next),
                        _ => None,
                    });
                    continue;
                }
            };
            if same_press(self.last_media, key, via, Instant::now()) {
                log::debug!("media key {key:?} by {via:?}: the same press");
                continue;
            }
            log::debug!("media key {key:?} by {via:?}");
            self.last_media = Some((key, via, Instant::now()));
            let cmd = match key {
                win::MediaKey::PlayPause => Some(Cmd::TogglePause),
                win::MediaKey::Play => paused.then_some(Cmd::TogglePause),
                win::MediaKey::Pause => (!paused).then_some(Cmd::TogglePause),
                win::MediaKey::Stop => Some(Cmd::Stop),
                win::MediaKey::Next => Some(Cmd::Next),
                win::MediaKey::Previous => Some(Cmd::Prev),
            };
            self.pending.extend(cmd.and_then(play));
        }
    }

    /// Keep the display on while a picture plays in sight and the computer
    /// awake while sound plays; let them sleep when paused, stopped or
    /// idle.
    /// The position as the taskbar button's progress (yellow when paused)
    /// and the thumbnail buttons' state.
    pub(super) fn show_on_taskbar(&mut self) {
        let Some(taskbar) = &mut self.taskbar else {
            return;
        };
        let state = self.player.as_ref().map(|p| &p.state);
        let progress = state
            .filter(|s| !s.idle)
            .and_then(|s| s.fraction())
            .map(|p| (p * 1000.0).round() as u64);
        let repeat = self.settings.repeat;
        taskbar.show(crate::taskbar::Shown {
            progress,
            paused: state.is_none_or(|s| s.paused || s.idle),
            has_previous: self.playlist.prev_in(repeat).is_some(),
            has_next: self.playlist.next_in(repeat).is_some(),
        });
    }

    /// Take the media keys as the setting says: Windows' media controls
    /// made or let go, the windows' own keys on or off.
    pub(super) fn follow_media_keys(&mut self) {
        use crate::settings::MediaKeys;
        let wanted = self.settings.media_keys;
        win::set_media_keys(wanted != MediaKeys::Off);
        if wanted == MediaKeys::Always {
            if self.smtc.is_none() && !self.smtc_failed {
                self.smtc = self.hwnd.and_then(crate::smtc::Smtc::new);
                self.smtc_failed = self.smtc.is_none();
            }
        } else if self.smtc.take().is_some() {
            log::debug!("media controls let go");
        }
    }

    /// What plays in Windows' media controls: the state, previous and
    /// next, the title (and the artist of a song).
    pub(super) fn show_in_media_controls(&mut self) {
        if self.smtc.is_none() {
            return;
        }
        use crate::smtc::Status;
        let state = self.state();
        let status = match &self.player {
            _ if self.engine_error.is_some() => Status::Closed,
            None => Status::Closed,
            Some(_) if state.idle && self.playlist.is_empty() => Status::Closed,
            Some(_) if state.idle => Status::Stopped,
            Some(_) if state.paused || state.eof => Status::Paused,
            Some(_) => Status::Playing,
        };
        let music = state.video().is_none();
        let shown = crate::smtc::Shown {
            status,
            has_previous: self.playlist.prev_in(self.settings.repeat).is_some(),
            has_next: self.playlist.next_in(self.settings.repeat).is_some(),
            title: if status == Status::Closed {
                String::new()
            } else {
                self.title(&state)
            },
            artist: music
                .then(|| crate::info::artist(&state.metadata).map(str::to_owned))
                .flatten(),
            music,
        };
        if let Some(smtc) = &mut self.smtc {
            smtc.show(shown);
        }
    }

    pub(super) fn follow_on_top(&mut self, ctx: &egui::Context) {
        let playing = self
            .player
            .as_ref()
            .is_some_and(|p| !p.state.idle && !p.state.paused && !p.state.eof);
        let on_top = self.mini.is_some()
            || match self.settings.on_top {
                OnTop::Never => false,
                OnTop::Always => true,
                OnTop::Playing => playing,
            };
        if on_top != self.on_top {
            self.on_top = on_top;
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }));
        }
    }

    pub(super) fn keep_awake(&mut self, minimized: bool) {
        let awake = match &self.player {
            Some(p) if !p.state.idle && !p.state.paused && !p.state.eof => {
                if p.state.video().is_some() && !minimized {
                    win::Awake::Display
                } else {
                    win::Awake::System
                }
            }
            _ => win::Awake::No,
        };
        if awake != self.awake {
            log::debug!("keep awake: {awake:?}");
            win::keep_awake(awake);
            self.awake = awake;
        }
    }

    /// Give mpv the chosen output device while Windows has it, else the
    /// default one; also when a device comes or goes, or the setting
    /// changes.
    pub(super) fn follow_audio_device(&mut self) {
        let Some(player) = &mut self.player else {
            return;
        };
        let Some(device) =
            crate::audio::device_to_use(&self.settings.audio_device, &player.state.audio_devices)
        else {
            return;
        };
        if device != self.audio_device_used {
            log::debug!(
                "audio device: {:?}",
                if device.is_empty() { "auto" } else { device }
            );
            player.set_audio_device(device);
            self.audio_device_used = device.to_owned();
        }
    }
}

/// Media key `key` at `at`, come by `via`, is the press `last` was, come
/// again another way: Play, Pause and Play/Pause are one button. Two quick
/// presses the same way are two.
pub(super) fn same_press(
    last: Option<(win::MediaKey, win::Via, Instant)>,
    key: win::MediaKey,
    via: win::Via,
    at: Instant,
) -> bool {
    use win::MediaKey::{Pause, Play, PlayPause};
    let button = |k: win::MediaKey| match k {
        Play | Pause | PlayPause => PlayPause,
        other => other,
    };
    last.is_some_and(|(k, way, t)| {
        way != via
            && button(k) == button(key)
            && at.saturating_duration_since(t) < Duration::from_millis(300)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_media_key_counts_once() {
        use win::MediaKey::{Next, Pause, PlayPause};
        use win::Via::{Command, Controls, Key};
        let t = Instant::now();
        let ms = |n| t + Duration::from_millis(n);
        // The window's Play/Pause, then the media controls' Pause.
        assert!(same_press(
            Some((PlayPause, Key, t)),
            Pause,
            Controls,
            ms(40)
        ));
        assert!(same_press(Some((Next, Controls, t)), Next, Key, ms(40)));
        // The window in front: WM_APPCOMMAND, then the key.
        assert!(same_press(
            Some((PlayPause, Command, t)),
            PlayPause,
            Key,
            ms(1)
        ));
        assert!(!same_press(
            Some((PlayPause, Key, t)),
            Pause,
            Controls,
            ms(500)
        ));
        assert!(!same_press(
            Some((PlayPause, Key, t)),
            Next,
            Controls,
            ms(40)
        ));
        // Twice quickly on the keyboard: two presses.
        assert!(!same_press(Some((Next, Key, t)), Next, Key, ms(40)));
        assert!(!same_press(Some((Next, Command, t)), Next, Command, ms(40)));
        assert!(!same_press(None, Next, Key, t));
    }
}
