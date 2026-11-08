//! Fullscreen: the panels shown near the edges, the title strip, the
//! playlist over the right side.

use super::*;

impl App {
    /// How long the mouse has rested: for ever before it first moves (egui
    /// gives infinity then, which `Duration::from_secs_f32` panics on).
    pub(super) fn pointer_rest(ctx: &egui::Context) -> Duration {
        rest_of(ctx.input(|i| i.pointer.time_since_last_movement()))
    }

    /// Which panels over the picture are shown in fullscreen, each on its
    /// own: the control bar while the pointer is near the bottom, a seek is
    /// dragged or the tracks window is open; the strip with the name (when
    /// it is on) while the pointer is near the top.
    pub(super) fn fullscreen_panels(&self, ctx: &egui::Context) -> Panels {
        let screen = ctx.content_rect();
        // Not over the playlist on the right.
        let pointer = ctx
            .pointer_hover_pos()
            .filter(|p| p.x < screen.right() - self.fullscreen_list_width);
        let (top, bottom) = Self::held(ctx, self.shown_over);
        Panels {
            top: self.settings.fullscreen_title
                && (top
                    || pointer
                        .is_some_and(|p| p.y < screen.top() + FULLSCREEN_TITLE_HEIGHT + 16.0)),
            bottom: bottom
                || pointer.is_some_and(|p| p.y > screen.bottom() - FULLSCREEN_BAR_ZONE)
                || self.seek_drag.is_some()
                || self.video_panel.open
                || self.tracks_window.open,
        }
    }

    /// The panels over the picture (the top, the bottom) of `shown`, the
    /// last frame's, that stay while a menu or a popup is open: the
    /// pointer went from them onto it.
    pub(super) fn held(ctx: &egui::Context, shown: (bool, bool)) -> (bool, bool) {
        if egui::Popup::is_any_open(ctx) {
            shown
        } else {
            (false, false)
        }
    }

    /// The control bar and the file's name over the picture in fullscreen
    /// ([`Self::fullscreen_panels`]). The pointer hides after a rest away
    /// from them, unless the information window is open.
    pub(super) fn fullscreen_controls(
        &mut self,
        ctx: &egui::Context,
        state: &State,
        title: &str,
        cmds: &mut Vec<Cmd>,
    ) {
        let panels = self.fullscreen_panels(ctx);
        self.shown_over = (panels.top, panels.bottom);
        let rest = Self::pointer_rest(ctx);
        if rest < HIDE_AFTER {
            ctx.request_repaint_after(HIDE_AFTER - rest);
        } else if !panels.top
            && !panels.bottom
            && !self.info.open
            && !self.video_panel.open
            && !self.tracks_window.open
            && !egui::Popup::is_any_open(ctx)
            && !self.fullscreen_playlist
        {
            ctx.set_cursor_icon(CursorIcon::None);
        }
        let alpha = (255.0 * (1.0 - self.settings.fullscreen_transparency / 100.0)).round() as u8;
        let fill = style::bar_bg().gamma_multiply_u8(alpha);
        self.fullscreen_list_width = if self.fullscreen_playlist {
            self.fullscreen_playlist_panel(
                ctx,
                state,
                style::panel_bg().gamma_multiply_u8(alpha),
                cmds,
            )
        } else {
            0.0
        };
        // The panels leave the playlist's side.
        let mut screen = ctx.content_rect();
        screen.max.x -= self.fullscreen_list_width;

        if panels.top {
            fullscreen_title(ctx, screen, title, fill);
        }
        if !panels.bottom {
            return;
        }
        let compact = self.compact_bar();
        let bar = self.controls_bar(state, true, compact);
        egui::Area::new(egui::Id::new("fullscreen_controls"))
            .order(egui::Order::Foreground)
            .anchor(Align2::LEFT_BOTTOM, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(fill)
                    .inner_margin(egui::Margin {
                        left: 0,
                        right: 0,
                        top: 6,
                        bottom: 4,
                    })
                    .show(ui, |ui| {
                        ui.set_width(screen.width());
                        cmds.extend(ui::controls::bar(ui, bar));
                    });
            });
    }

    /// The playlist over the picture's right side in fullscreen, as tall as
    /// the screen, on `fill`; its left edge dragged sets its width (kept in
    /// egui memory, apart from the window's playlist). Its width.
    pub(super) fn fullscreen_playlist_panel(
        &mut self,
        ctx: &egui::Context,
        state: &State,
        fill: Color32,
        cmds: &mut Vec<Cmd>,
    ) -> f32 {
        const GRIP: f32 = 6.0;
        let screen = ctx.content_rect();
        let id = egui::Id::new("fullscreen_playlist_width");
        let most = (screen.width() * 0.6).max(ui::playlist::MIN_WIDTH);
        let width = ctx
            .data_mut(|d| *d.get_persisted_mut_or(id, 320.0_f32))
            .clamp(ui::playlist::MIN_WIDTH, most);
        let rect = Rect::from_min_max(egui::pos2(screen.right() - width, screen.top()), screen.max);
        let logos = self.settings.channel_logos.then_some(&mut self.logos);
        let (playlist, repeat) = (&mut self.playlist, self.settings.repeat);
        let groups = self.settings.playlist_groups;
        let progress = self.settings.playlist_progress;
        let (list_state, positions) = (&mut self.list_state, &self.positions);
        egui::Area::new(egui::Id::new("fullscreen_playlist"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                egui::Frame::NONE.fill(fill).inner_margin(6).show(ui, |ui| {
                    let inner = rect.size() - egui::vec2(12.0, 12.0);
                    ui.set_min_size(inner);
                    ui.set_max_size(inner);
                    cmds.extend(ui::playlist::panel(
                        ui,
                        ui::playlist::Panel {
                            playlist,
                            state: list_state,
                            positions,
                            logos,
                            repeat,
                            groups,
                            progress,
                            chapters: &state.chapters,
                            time: state.time,
                            duration: state.duration,
                            detached: false,
                        },
                    ));
                });
                // The left edge resizes it.
                let grip = Rect::from_min_size(rect.min, egui::vec2(GRIP, rect.height()));
                let response = ui.interact(grip, id.with("grip"), Sense::drag());
                if response.hovered() || response.dragged() {
                    ctx.set_cursor_icon(CursorIcon::ResizeHorizontal);
                }
                if response.dragged() {
                    let wider =
                        (width - response.drag_delta().x).clamp(ui::playlist::MIN_WIDTH, most);
                    ctx.data_mut(|d| d.insert_persisted(id, wider));
                }
            });
        width
    }
}

/// The panels over the picture in fullscreen that are shown.
pub(super) struct Panels {
    /// The strip with the file's name.
    pub(super) top: bool,
    /// The control bar.
    pub(super) bottom: bool,
}

/// The strip with the file's name across the top in fullscreen. It takes no
/// clicks, so a double click there still leaves fullscreen.
pub(super) fn fullscreen_title(ctx: &egui::Context, screen: Rect, title: &str, fill: Color32) {
    egui::Area::new(egui::Id::new("fullscreen_title"))
        .order(egui::Order::Foreground)
        .interactable(false)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(screen.width(), FULLSCREEN_TITLE_HEIGHT),
                Sense::hover(),
            );
            ui.painter().rect_filled(rect, 0.0, fill);
            let galley = ui.fonts_mut(|f| {
                let mut job = egui::text::LayoutJob::simple_singleline(
                    title.to_owned(),
                    FontId::proportional(14.0),
                    style::text(),
                );
                job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 48.0);
                f.layout_job(job)
            });
            let pos = Align2::CENTER_CENTER
                .anchor_size(rect.center(), galley.size())
                .min;
            ui.painter().galley(pos, galley, style::text());
        });
}

/// The pointer is in `zone` and over nothing but the window's own panels
/// (no menu, popup or window of egui).
pub(super) fn pointer_over(ctx: &egui::Context, zone: Rect) -> bool {
    ctx.pointer_hover_pos().is_some_and(|p| {
        zone.contains(p)
            && ctx
                .layer_id_at(p)
                .is_none_or(|layer| layer.order == egui::Order::Background)
    })
}
