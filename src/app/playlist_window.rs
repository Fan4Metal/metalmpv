//! The playlist in a window of its own (`dock.rs`).

use super::*;

impl App {
    /// The playlist in a window of its own (`dock.rs`) while it is shown
    /// and detached: an egui viewport drawn in this frame, as metalmd's
    /// detached tabs are; not in fullscreen when it is over the player's
    /// screen. `keys`: the player's keys work (no modal is open).
    pub(super) fn playlist_window(
        &mut self,
        ctx: &egui::Context,
        state: &State,
        cmds: &mut Vec<Cmd>,
        keys: bool,
    ) {
        let player = self.hwnd.and_then(win::window_rect);
        let over_player = match (player, self.settings.playlist_rect) {
            (Some(m), Some(p)) => p[0] < m[2] && m[0] < p[2] && p[1] < m[3] && m[1] < p[3],
            _ => false,
        };
        let shown = self.settings.playlist_visible
            && self.settings.playlist_detached
            && self.interface() != Interface::Bare
            && self.mini.is_none()
            && !(self.fullscreen && over_player);
        if !shown {
            if self.playlist_builder.take().is_some() {
                self.playlist_hwnd = None;
                self.playlist_looked = 0;
                win::playlist_closed();
            }
            return;
        }
        let ppp = ctx.pixels_per_point();
        let saved = self
            .settings
            .playlist_rect
            .filter(|r| win::on_a_monitor(*r));
        let height = ctx.content_rect().height();
        let accent = self.settings.accent;
        let builder = self
            .playlist_builder
            .get_or_insert_with(|| {
                // Frameless as the player's: the title row is the app's own.
                // Made hidden: Windows showed a new window white until its
                // first frame. It is shown, cloaked, once found below.
                let builder = egui::ViewportBuilder::default()
                    .with_visible(false)
                    .with_title(tr!("Playlist", "Плейлист"))
                    .with_decorations(false)
                    .with_resizable(true)
                    .with_min_inner_size([ui::playlist::MIN_WIDTH, 160.0])
                    .with_minimize_button(false)
                    .with_maximize_button(false)
                    .with_drag_and_drop(true)
                    .with_taskbar(false)
                    .with_icon(crate::window_icon(accent));
                // Near enough: the window is put exactly once it is found.
                match saved {
                    Some(r) => builder
                        .with_position(egui::pos2(r[0] as f32 / ppp, r[1] as f32 / ppp))
                        .with_inner_size([(r[2] - r[0]) as f32 / ppp, (r[3] - r[1]) as f32 / ppp]),
                    None => builder.with_inner_size([PLAYLIST_WIDTH, height]),
                }
            })
            .clone();
        let title = builder.title.clone().unwrap_or_default();
        let id = egui::ViewportId::from_hash_of("playlist_window");
        let mut close = false;
        let mut dropped = Vec::new();
        ctx.show_viewport_immediate(id, builder, |ui, _class| {
            let ctx = ui.ctx().clone();
            close = ctx.input(|i| i.viewport().close_requested());
            dropped = dropped_files(&ctx);
            let pasted = win::take_paste(win::PasteIn::Playlist);
            if keys && !ui::playlist::typing(&ctx) && !egui::Popup::is_any_open(&ctx) {
                if pasted {
                    cmds.push(Cmd::Paste);
                }
                if let Some(id) = ctx.memory(|m| m.focused()) {
                    ctx.memory_mut(|m| m.surrender_focus(id));
                }
                // Esc here closes this window, not the program; so does
                // the playlist's key (a player in fullscreen would show
                // its own playlist for it).
                for cmd in input::keys(&ctx, &self.settings.keys) {
                    match cmd {
                        Cmd::Escape | Cmd::TogglePlaylist => close = true,
                        cmd => cmds.push(cmd),
                    }
                }
            }
            frameless::resize_edges(ui);
            egui::Panel::top("playlist_title")
                .exact_size(style::TITLE_HEIGHT)
                .frame(egui::Frame::NONE.fill(style::title_bg()))
                .show_separator_line(false)
                .show(ui, |ui| {
                    close |= ui::title::tool_row(ui, tr!("Playlist", "Плейлист"));
                });
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(style::panel_bg()).inner_margin(6))
                .show(ui, |ui| {
                    let logos = self.settings.channel_logos.then_some(&mut self.logos);
                    cmds.extend(ui::playlist::panel(
                        ui,
                        ui::playlist::Panel {
                            playlist: &mut self.playlist,
                            state: &mut self.list_state,
                            positions: &self.positions,
                            logos,
                            repeat: self.settings.repeat,
                            groups: self.settings.playlist_groups,
                            progress: self.settings.playlist_progress,
                            chapters: &state.chapters,
                            time: state.time,
                            duration: state.duration,
                            detached: true,
                        },
                    ));
                });
        });
        self.drop_files(dropped, state);
        if close {
            self.settings.playlist_visible = false;
        }
        // eframe gives no handle for the window: the one of this thread with
        // its title, hidden as it is made. Found again if it was made anew
        // (its handle may then be the old one's: not owned by the player
        // yet).
        if let (Some(h), Some(main)) = (self.playlist_hwnd, self.hwnd)
            && !win::owned_by(h, main)
        {
            self.playlist_hwnd = None;
        }
        if self.playlist_hwnd.is_none() {
            self.playlist_looked += 1;
            // Not found: shown as it is rather than never.
            if self.playlist_looked == PLAYLIST_LOOKED {
                ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Visible(true));
            }
            ctx.request_repaint();
        }
        if self.playlist_hwnd.is_none()
            && let Some(main) = self.hwnd
            && let Some(child) = win::thread_window(&title).filter(|h| *h != main)
        {
            self.playlist_hwnd = Some(child);
            // Shown cloaked, uncloaked once it has painted.
            win::cloak(child, true);
            self.playlist_painted = 0;
            let first = || {
                let main = win::visible_rect(main)?;
                Some(crate::dock::first_place(
                    main,
                    (PLAYLIST_WIDTH * ppp).round() as i32,
                    self.hwnd.and_then(win::work_area),
                ))
            };
            if let Some(rect) = saved.or_else(first) {
                win::set_visible_rect(child, rect);
            }
            win::adopt_playlist(child, main);
            ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Visible(true));
        }
        if let Some(h) = self.playlist_hwnd
            && self.playlist_painted < PLAYLIST_PAINTED
        {
            self.playlist_painted += 1;
            if self.playlist_painted == PLAYLIST_PAINTED {
                win::cloak(h, false);
            }
            ctx.request_repaint();
        }
        // A minimised window is far off the screen.
        if let Some(rect) = self.playlist_hwnd.and_then(win::visible_rect)
            && rect[0] > -30000
        {
            self.settings.playlist_rect = Some(rect);
        }
    }
}

/// The width of the playlist's own window the first time, in points.
pub(super) const PLAYLIST_WIDTH: f32 = 300.0;

/// Frames the playlist's window is looked for before it is shown anyway,
/// and frames it paints, cloaked, before it is uncloaked.
pub(super) const PLAYLIST_LOOKED: u32 = 10;

pub(super) const PLAYLIST_PAINTED: u32 = 3;
