//! The video area: the picture and its pointer, the navigator in its
//! corner, or the placeholder: the start screen, an audio file's cover.

use super::*;

impl App {
    /// The picture, or what stands in for it; the mouse on it.
    pub(super) fn video_area(
        &mut self,
        ui: &mut egui::Ui,
        frame: &mut eframe::Frame,
        state: &State,
        cmds: &mut Vec<Cmd>,
    ) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
        // A double click switches fullscreen, a drag moves the window.
        let response = frameless::drag_area(ui, rect, ui.id().with("video"), Sense::CLICK);
        let visible = state.video().is_some() && self.video.is_some();
        self.recent_scroll = None;
        if !state.idle
            && let Some(video) = &mut self.video
        {
            video.show(ui, frame, rect, visible);
        }
        if visible {
            if !self.first_frame_logged {
                self.first_frame_logged = true;
                log::debug!("video size known at {:.0} ms", crate::since_start());
            }
        } else {
            if let Some(cmd) = self.placeholder(ui, rect, state) {
                cmds.push(cmd);
            }
        }
        // After the drag area: over it, it takes the pointer.
        if visible && self.settings.navigator {
            self.corner_navigator(ui, rect, state);
        }
        // In the mini player, the player's window again.
        if ui::double_clicked(&response) {
            cmds.push(if self.mini.is_some() {
                Cmd::ToggleMini
            } else {
                Cmd::ToggleFullscreen
            });
        }
        // The middle button shows or hides the playlist when let go, however
        // the pointer moves meanwhile, as the right one opens the menu
        // (egui's click allows 6 points: a click on the move was lost).
        let (middle_pressed, middle_down) = ui.input(|i| {
            (
                i.pointer.button_pressed(egui::PointerButton::Middle),
                i.pointer.button_down(egui::PointerButton::Middle),
            )
        });
        if middle_pressed {
            self.middle_press = response.contains_pointer();
        }
        if self.middle_press && !middle_down {
            self.middle_press = false;
            cmds.push(Cmd::TogglePlaylist);
        }
        let on_recent = self
            .recent_scroll
            .is_some_and(|r| ui.rect_contains_pointer(r));
        if response.hovered() && !on_recent {
            let notches = ui::wheel_notches(ui);
            if notches != 0 {
                cmds.push(Cmd::Volume(notches as f64 * input::VOLUME_STEP));
            }
        }
        // In only the picture the playlist is not shown: the menu offers it.
        let playlist_visible = self.playlist_shown(ui.ctx());
        let chapter_marks = self.settings.chapter_marks;
        let marks = Marks {
            fill: self.fill,
            interface: self.interface(),
            mini: self.mini.is_some(),
            aspect: self.adjust.aspect,
            repeat: self.settings.repeat,
            on_top: self.settings.on_top,
            playlist_detached: self.settings.playlist_detached,
            info_overlay: self.settings.info_overlay,
            bookmarks: self
                .settings
                .bookmarks
                .iter()
                .map(|b| {
                    let here = self.playlist.current_path().is_some_and(|p| b.is_of(p));
                    // A bookmark of an older version: the duration of its
                    // file if it plays.
                    let share = b.share().or_else(|| {
                        let duration = state.duration.filter(|d| here && *d > 0.0)?;
                        Some((b.time / duration).clamp(0.0, 1.0) as f32)
                    });
                    let title = self
                        .playlist
                        .channel(&b.path)
                        .and_then(|c| c.title.as_deref());
                    (b.label(title), here, share)
                })
                .collect(),
        };
        // As Windows' menus do: the right button opens the menu when let go
        // and a press anywhere off the menus closes it, however the pointer
        // moves meanwhile (egui's click allows 6 points: a click on the
        // move was lost). The press closes it before it is shown, so that
        // the right button let go there opens it anew, once the old one is
        // gone (egui closes a menu shown in the last frame on any click).
        let menu = egui::Popup::default_response_id(&response);
        let ctx = ui.ctx().clone();
        let (pressed, right_pressed, right_down, pos) = ui.input(|i| {
            (
                i.pointer.any_pressed(),
                i.pointer.button_pressed(egui::PointerButton::Secondary),
                i.pointer.button_down(egui::PointerButton::Secondary),
                i.pointer.interact_pos(),
            )
        });
        let on_menus = pos
            .and_then(|p| ctx.layer_id_at(p))
            .is_some_and(|layer| layer.order == egui::Order::Foreground);
        if pressed && !on_menus && egui::Popup::is_id_open(&ctx, menu) {
            egui::Popup::close_id(&ctx, menu);
        }
        if right_pressed {
            self.menu_press = response.contains_pointer();
        }
        let let_go = self.menu_press && !right_down;
        let old_shown = ctx.read_response(menu).is_some();
        if let_go && old_shown {
            ctx.request_repaint();
        }
        let open = let_go && !old_shown;
        if open {
            self.menu_press = false;
        }
        egui::Popup::menu(&response)
            .open_memory(open.then_some(egui::SetOpenCommand::Bool(true)))
            .at_pointer_fixed()
            .show(|ui| context_menu(ui, state, playlist_visible, chapter_marks, marks, cmds));
        // Subtitles alone over a playing file are added to it (`drop_files`).
        let hovered = ui.input(|i| {
            let files = &i.raw.hovered_files;
            (!files.is_empty()).then(|| {
                !state.idle
                    && files
                        .iter()
                        .all(|f| f.path.as_deref().is_some_and(tracks::is_subtitles))
            })
        });
        if let Some(only_subtitles) = hovered {
            ui.painter()
                .rect_filled(rect.shrink(12.0), 8.0, Color32::from_black_alpha(160));
            ui.painter().rect_stroke(
                rect.shrink(12.0),
                8.0,
                egui::Stroke::new(2.0, style::accent()),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                if only_subtitles {
                    tr!(
                        "Drop to load the subtitles",
                        "Отпустите, чтобы загрузить субтитры"
                    )
                } else if self.playlist.is_empty() {
                    tr!("Drop to play", "Отпустите, чтобы воспроизвести")
                } else {
                    tr!(
                        "Drop to add to the playlist",
                        "Отпустите, чтобы добавить в плейлист"
                    )
                },
                FontId::proportional(20.0),
                style::TEXT_BRIGHT,
            );
        }
        // In fullscreen the line goes under the strip with the name while
        // that is shown.
        let osd_rect = if self.fullscreen && self.fullscreen_panels(ui.ctx()).top {
            rect.with_min_y(rect.top() + FULLSCREEN_TITLE_HEIGHT)
        } else if self.title_strip {
            rect.with_min_y(rect.top() + style::TITLE_HEIGHT)
        } else {
            rect
        };
        let scale = (self.settings.osd_scale / 100.0) as f32;
        self.osd.set_duration(self.settings.osd_duration);
        let look = &self.settings.osd_style;
        self.osd
            .paint(ui.ctx(), osd_rect, state, look, scale, self.settings.osd);
        // The window says it all while open; the mini player shows only the
        // picture.
        if self.settings.info_overlay
            && !self.info.open
            && self.mini.is_none()
            && !state.idle
            && self.engine_error.is_none()
        {
            let mut area = osd_rect;
            if self.fullscreen {
                area.max.x -= self.fullscreen_list_width;
            }
            let subject = info_subject(&self.playlist, self.facts.as_ref(), state);
            let lang = crate::i18n::lang();
            let rows = if self.settings.info_overlay_brief {
                vec![crate::info::brief_chips_in(lang, &subject)]
            } else {
                crate::info::chips_in(lang, &subject, crate::info::Unfolded::default())
            };
            let transparency = self.settings.info_overlay_transparency as f32;
            ui::info::overlay(ui.ctx(), area, &rows, transparency);
        }
    }

    /// The navigator over the picture's bottom right corner while the
    /// picture is zoomed in past `rect`, above where the control bar comes
    /// over the picture (the pointer there would bring the bar over it)
    /// and off the fullscreen playlist.
    pub(super) fn corner_navigator(&mut self, ui: &mut egui::Ui, rect: Rect, state: &State) {
        const MARGIN: f32 = 12.0;
        let geometry = ui::navigator::Geometry::new(self.picture_size(state), rect.size());
        if !geometry.zoomed_in(&self.adjust) {
            return;
        }
        let (bottom, right) = if self.fullscreen {
            (FULLSCREEN_BAR_ZONE, self.fullscreen_list_width)
        } else if self.mini.is_some() {
            (ui::controls::MINI_ROW + 8.0, 0.0)
        } else if self.interface() == Interface::Bare {
            (ui::controls::height(self.compact_bar()) + 16.0, 0.0)
        } else {
            (0.0, 0.0)
        };
        let size = geometry.size(&self.adjust, egui::vec2(160.0, 110.0));
        let corner = egui::pos2(
            rect.right() - right - MARGIN,
            rect.bottom() - bottom - MARGIN,
        );
        let navigator = Rect::from_min_max(corner - size, corner);
        // Not in a window too small for it.
        if !rect.shrink(MARGIN).contains_rect(navigator) {
            return;
        }
        let mut adjust = self.adjust.clone();
        let id = egui::Id::new("corner_navigator");
        let ground = Color32::from_black_alpha(160);
        if ui::navigator::show(ui, navigator, id, geometry, &mut adjust, ground) {
            self.change_picture(|a| *a = adjust);
        }
    }

    /// The black area without a picture: the start screen, the title of an
    /// audio file, or why nothing plays.
    pub(super) fn placeholder(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        state: &State,
    ) -> Option<Cmd> {
        let failed = self.engine_error.is_some() || (state.idle && self.file_error.is_some());
        if state.idle && !failed {
            return self.start_screen(ui, rect);
        }
        if !failed && self.audio_cover(ui, rect, state) {
            return None;
        }
        let size = start_icon_size(rect);
        let icon = ui::app_icon(ui.ctx(), size, &mut self.start_icon);
        let painter = ui.painter_at(rect);
        let tint = if state.idle {
            Color32::from_white_alpha(70)
        } else {
            Color32::WHITE
        };
        let centre = rect.center() - egui::vec2(0.0, size * 0.3);
        let icon_rect = ui::paint_icon(&painter, &icon, centre, tint);
        let below = icon_rect.center_bottom() + egui::vec2(0.0, 14.0);
        let (text, color) = if let Some(e) = &self.engine_error {
            (e.message(), style::ERROR)
        } else if let Some(e) = self.file_error.as_ref().filter(|_| state.idle) {
            (
                tr!(
                    format!("Cannot play the file: {e}"),
                    format!("Не удалось воспроизвести файл: {e}")
                ),
                style::ERROR,
            )
        } else {
            // An audio file: its title.
            (state.title.clone().unwrap_or_default(), style::text())
        };
        // Wrapped to the area, each line centred: an error may be long.
        let mut job = egui::text::LayoutJob::simple(
            text,
            FontId::proportional(14.0),
            color,
            (rect.width() - 48.0).clamp(120.0, 640.0),
        );
        job.halign = egui::Align::Center;
        let galley = painter.layout_job(job);
        let at = below - egui::vec2(galley.rect.center().x, galley.rect.top());
        painter.galley(at, galley, color);
        None
    }

    /// Nothing plays: the icon, the name, the version and what to do, as
    /// in qview; centred, a little above the middle.
    pub(super) fn start_screen(&mut self, ui: &mut egui::Ui, rect: Rect) -> Option<Cmd> {
        let size = start_icon_size(rect);
        let big = size > 64.0;
        let icon = ui::app_icon(ui.ctx(), size, &mut self.start_icon);
        let painter = ui.painter_at(rect);
        let layout = |text: String, points: f32, color: Color32| {
            painter.layout_no_wrap(text, FontId::proportional(points), color)
        };
        let name = layout(
            crate::APP_NAME.to_owned(),
            if big { 30.0 } else { 22.0 },
            style::TEXT_BRIGHT,
        );
        let version = layout(
            tr!(
                format!("Version {}", crate::VERSION),
                format!("Версия {}", crate::VERSION)
            ),
            13.0,
            style::text_weak(),
        );
        let open = keymap::shortcut(Cmd::Open);
        let hint = if open.is_empty() {
            tr!("Drop a file here", "Перетащите файл сюда").to_owned()
        } else {
            tr!(
                format!("Drop a file here, or open one with {open}"),
                format!("Перетащите файл сюда или откройте его через {open}")
            )
        };
        let hint = layout(hint, 14.0, style::text());
        const GAPS: [f32; 3] = [14.0, 2.0, 18.0];
        let base =
            size + GAPS.iter().sum::<f32>() + name.size().y + version.size().y + hint.size().y;
        // The recent files under the hint, as many as fit.
        const RECENT_ROW: f32 = 36.0;
        const RECENT_EXTRA: f32 = 32.0 + 20.0 + 8.0 + 22.0;
        let fit = ((rect.height() - 16.0 - base - RECENT_EXTRA) / RECENT_ROW).floor();
        let shown = self.settings.recent.len().min(fit.max(0.0) as usize).min(8);
        let height = if shown > 0 {
            base + RECENT_EXTRA + shown as f32 * RECENT_ROW
        } else {
            base
        };
        let top = (rect.center().y - height * 0.55).max(rect.top() + 8.0);
        let icon_rect = ui::paint_icon(
            &painter,
            &icon,
            egui::pos2(rect.center().x, top + size / 2.0),
            Color32::WHITE,
        );
        let mut y = icon_rect.bottom() + GAPS[0];
        for (galley, gap) in [(name, GAPS[1]), (version, GAPS[2]), (hint, 0.0)] {
            let h = galley.size().y;
            let x = rect.center().x - galley.size().x / 2.0;
            painter.galley(egui::pos2(x, y), galley, style::text());
            y += h + gap;
        }
        if shown == 0 {
            return None;
        }
        let mut out = None;
        y += 32.0;
        // Centred under the hint, as in metalmd.
        let width = (rect.width() - 48.0).clamp(120.0, 520.0);
        let centre = rect.center().x;
        let centred = |y: f32, size: egui::Vec2| {
            Rect::from_min_size(egui::pos2(centre - size.x / 2.0, y), size)
        };
        let header = layout(
            tr!("RECENT", "НЕДАВНИЕ").to_owned(),
            11.0,
            style::text_weak(),
        );
        painter.galley(centred(y, header.size()).min, header, style::text_weak());
        y += 20.0 + 8.0;
        let truncated = |text: String, points: f32, color: Color32| {
            let mut job =
                egui::text::LayoutJob::simple_singleline(text, FontId::proportional(points), color);
            job.wrap = egui::text::TextWrapping::truncate_at_width(width);
            painter.layout_job(job)
        };
        // As many rows as fit are seen; the rest scroll in the same place.
        let list_rect = Rect::from_min_size(
            egui::pos2(centre - width / 2.0, y),
            egui::vec2(width, shown as f32 * RECENT_ROW),
        );
        if self.settings.recent.len() > shown {
            self.recent_scroll = Some(list_rect);
        }
        let mut list = ui.new_child(egui::UiBuilder::new().max_rect(list_rect));
        egui::ScrollArea::vertical()
            .id_salt("recent_files")
            .max_height(list_rect.height())
            .auto_shrink([false, false])
            // The wheel scrolls it; a bar would only crowd the centred rows.
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(&mut list, |ui| {
                // Rows of exactly `RECENT_ROW`: with egui's spacing between
                // them the last one seen was cut.
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, path) in self.settings.recent.iter().enumerate() {
                    // A place in the column, not a widget: the gaps still
                    // move the window.
                    let (_, row) = ui.allocate_space(egui::vec2(width, RECENT_ROW));
                    let name = path.file_name().map_or_else(
                        || path.display().to_string(),
                        |n| n.to_string_lossy().into_owned(),
                    );
                    let folder = path
                        .parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default();
                    // Laid out in the text colour: the hover only tints it.
                    let name = truncated(name, 14.0, style::text());
                    let folder = truncated(folder, 11.0, style::text_weak());
                    let name_rect = centred(row.top(), name.size());
                    let folder_rect = centred(name_rect.bottom() + 1.0, folder.size());
                    // The name and the folder under it take the click.
                    let response = ui
                        .interact(
                            name_rect.union(folder_rect),
                            ui.id().with(("recent", i)),
                            Sense::CLICK,
                        )
                        .on_hover_text(path.display().to_string());
                    let hovered = response.hovered();
                    if hovered {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    let color = if hovered {
                        style::accent()
                    } else {
                        style::text()
                    };
                    let painter = ui.painter();
                    painter.galley_with_override_text_color(name_rect.min, name, color);
                    painter.galley(folder_rect.min, folder, style::text_weak());
                    if response.clicked() {
                        out = Some(Cmd::OpenRecent(i));
                    }
                }
            });
        y = list_rect.bottom();
        let clear = layout(
            tr!("Clear the list", "Очистить список").to_owned(),
            12.0,
            style::text_weak(),
        );
        let clear_rect = centred(y + 4.0, clear.size());
        let response = ui.interact(clear_rect, ui.id().with("recent_clear"), Sense::CLICK);
        let color = if response.hovered() {
            style::accent()
        } else {
            style::text_weak()
        };
        painter.galley_with_override_text_color(clear_rect.min, clear, color);
        if response.clicked() {
            out = Some(Cmd::ClearRecent);
        }
        out
    }

    /// An audio file with a cover: the cover as big as fits, the title, the
    /// artist and the album under it. Whether there was one.
    pub(super) fn audio_cover(&self, ui: &egui::Ui, rect: Rect, state: &State) -> bool {
        let has_video = state
            .tracks
            .iter()
            .any(|t| t.kind == tracks::Kind::Video && !t.albumart);
        let Some((_, texture)) = self
            .cover
            .as_ref()
            .filter(|_| !has_video)
            .and_then(|c| c.loaded.as_ref())
        else {
            return false;
        };
        let painter = ui.painter_at(rect);
        let title = crate::info::tag(&state.metadata, &["title"])
            .map(str::to_owned)
            .or_else(|| state.title.clone())
            .unwrap_or_default();
        let mut lines = vec![(title, 18.0, style::TEXT_BRIGHT)];
        if let Some(artist) = crate::info::artist(&state.metadata) {
            lines.push((artist.to_owned(), 15.0, style::text()));
        }
        if let Some(album) = crate::info::album(&state.metadata) {
            lines.push((album, 14.0, style::text_weak()));
        }
        let galleys: Vec<_> = lines
            .into_iter()
            .map(|(text, size, color)| {
                ui.fonts_mut(|f| {
                    let mut job = egui::text::LayoutJob::simple_singleline(
                        text,
                        FontId::proportional(size),
                        color,
                    );
                    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 48.0);
                    f.layout_job(job)
                })
            })
            .collect();
        const GAP: f32 = 18.0;
        let text_height: f32 = galleys.iter().map(|g| g.size().y + 4.0).sum();
        let room = egui::vec2(
            rect.width() - 64.0,
            (rect.height() - 64.0 - GAP - text_height).min(rect.height() * 0.7),
        );
        if room.x < 32.0 || room.y < 32.0 {
            return false;
        }
        let picture = texture.size_vec2();
        let scale = (room.x / picture.x).min(room.y / picture.y);
        let size = picture * scale;
        let top = rect.center().y - (size.y + GAP + text_height) / 2.0;
        let image = Rect::from_min_size(egui::pos2(rect.center().x - size.x / 2.0, top), size);
        painter.image(
            texture.id(),
            image,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        let mut y = image.bottom() + GAP;
        for galley in galleys {
            let h = galley.size().y;
            let x = rect.center().x - galley.size().x / 2.0;
            painter.galley(egui::pos2(x, y), galley, style::text());
            y += h + 4.0;
        }
        true
    }
}

/// The icon of the start screen in points: 112 as in qview, 64 in a low
/// window (both embedded at 100–200 %).
pub(super) fn start_icon_size(rect: Rect) -> f32 {
    if rect.height() >= 340.0 { 112.0 } else { 64.0 }
}
