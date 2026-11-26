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
        if !state.idle && self.compare.is_some() && visible {
            self.compare_picture(ui, frame, rect, &response, state);
        } else if !state.idle
            && let Some(video) = &mut self.video
        {
            video.show(ui, frame, rect, visible);
            // No picture of A's yet: B's frames are taken unseen.
            if let Some(c) = &self.compare {
                c.video.skip_unseen();
            }
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
        // While comparing it drags the picture instead.
        let comparing = self.compare.is_some() && visible && !state.idle;
        let (middle_pressed, middle_down, latest) = ui.input(|i| {
            (
                i.pointer.button_pressed(egui::PointerButton::Middle),
                i.pointer.button_down(egui::PointerButton::Middle),
                i.pointer.latest_pos(),
            )
        });
        if middle_pressed {
            self.middle_press = response.contains_pointer();
            self.pan_drag = latest
                .filter(|_| self.middle_press && comparing)
                .map(|p| (p, [self.adjust.pan_x, self.adjust.pan_y]));
        }
        if let Some((from, start)) = self.pan_drag {
            if middle_down && comparing {
                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                if let Some(to) = latest {
                    self.pan_at_pointer(rect, from, start, to, state);
                }
            } else {
                self.pan_drag = None;
                self.middle_press = false;
            }
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
            let pointer = ui.input(|i| i.pointer.hover_pos());
            if notches == 0 {
            } else if let Some(pos) = pointer.filter(|_| self.compare.is_some() && visible) {
                // Comparing, the wheel zooms about the point under it.
                self.zoom_at_pointer(rect, pos, notches, state);
            } else {
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
            compare: (self.settings.compare_mode, self.compare.is_some()),
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
        // One video over a playing one: a second zone compares them.
        let hovered = ui.input(|i| i.raw.hovered_files.clone());
        self.compare_drop = None;
        if !hovered.is_empty() {
            let only_subtitles = !state.idle
                && hovered
                    .iter()
                    .all(|f| f.path.as_deref().is_some_and(tracks::is_subtitles));
            let text = if only_subtitles {
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
            };
            let area = rect.shrink(12.0);
            let painter = ui.painter();
            if self.can_compare_drop(&hovered, state) {
                // egui's pointer stands still while a file is dragged: the
                // zone under it is looked for meanwhile.
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(30));
                const GAP: f32 = 12.0;
                let (add, compare) = if area.width() >= area.height() {
                    let mid = area.center().x;
                    (
                        area.with_max_x(mid - GAP / 2.0),
                        area.with_min_x(mid + GAP / 2.0),
                    )
                } else {
                    let mid = area.center().y;
                    (
                        area.with_max_y(mid - GAP / 2.0),
                        area.with_min_y(mid + GAP / 2.0),
                    )
                };
                let on_compare = self
                    .drop_pointer(ui.ctx())
                    .is_some_and(|p| compare.contains(p));
                let a = self
                    .playlist
                    .current_path()
                    .map(|p| self.playlist.title(p))
                    .unwrap_or_default();
                drop_zone(painter, add, text, None, !on_compare);
                let compare_text = tr!("Drop to compare with", "Отпустите, чтобы сравнить с");
                drop_zone(painter, compare, compare_text, Some(&a), on_compare);
                self.compare_drop = Some(compare);
            } else {
                drop_zone(painter, area, text, None, true);
            }
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
        // The comparison's bar, off the fullscreen playlist and under the
        // zone that brings a strip over the picture's top, whether it is
        // shown or not: put under a strip shown, the bar left the zone, the
        // strip went and the bar jumped up into it again.
        if comparing && hovered.is_empty() && self.mini.is_none() {
            let mut area = rect;
            if self.fullscreen {
                area.max.x -= self.fullscreen_list_width;
            }
            let top = if self.fullscreen && self.settings.fullscreen_title {
                rect.top() + FULLSCREEN_TITLE_HEIGHT + 16.0
            } else if !self.fullscreen && self.interface().title_over() {
                rect.top() + style::TITLE_HEIGHT + 8.0
            } else {
                rect.top()
            };
            self.compare_bar(ui, area, top, cmds);
        }
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
            let brief = self.settings.info_overlay_brief;
            let rows = |subject: &crate::info::Subject| {
                if brief {
                    vec![crate::info::brief_chips_in(lang, subject)]
                } else {
                    crate::info::chips_in(lang, subject, crate::info::Unfolded::default())
                }
            };
            let transparency = self.settings.info_overlay_transparency as f32;
            // Two videos compared: each one's chips on its own side.
            let compared = self
                .compare
                .as_ref()
                .filter(|c| comparing && c.loaded && c.player.state.video().is_some());
            match compared {
                Some(c) => {
                    let b = crate::info::Subject {
                        path: Some(&c.path),
                        state: &c.player.state,
                        facts: c
                            .facts
                            .as_ref()
                            .filter(|f| f.path == c.path)
                            .and_then(|f| f.loaded.as_ref()),
                    };
                    let [(a_area, a_side), (b_area, b_side)] =
                        self.compare_chip_places(rect, area, state);
                    for (subject, area, side) in [(&subject, a_area, a_side), (&b, b_area, b_side)]
                    {
                        let rows = rows(subject);
                        ui::info::overlay(ui.ctx(), area, &rows, transparency, side, 0.9);
                    }
                }
                None => {
                    let rows = rows(&subject);
                    let side = egui::Align::Max;
                    ui::info::overlay(ui.ctx(), area, &rows, transparency, side, 0.6);
                }
            }
        }
    }

    /// The navigator over the picture's bottom right corner while the
    /// picture is zoomed in past `rect`, above where the control bar comes
    /// over the picture (the pointer there would bring the bar over it)
    /// and off the fullscreen playlist.
    pub(super) fn corner_navigator(&mut self, ui: &mut egui::Ui, rect: Rect, state: &State) {
        const MARGIN: f32 = 12.0;
        // Side by side, A's half.
        let area = self.picture_place(rect, None, state);
        let geometry = ui::navigator::Geometry::new(self.picture_size(state), area.size());
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

/// A place to drop files on: a dark ground, its frame in the accent colour
/// while the pointer is there (`lit`), the text in the middle and a name
/// under it.
fn drop_zone(painter: &egui::Painter, area: Rect, text: &str, name: Option<&str>, lit: bool) {
    let (stroke, color) = if lit {
        (egui::Stroke::new(2.0, style::accent()), style::TEXT_BRIGHT)
    } else {
        (egui::Stroke::new(1.0, style::line()), style::text_weak())
    };
    painter.rect_filled(
        area,
        8.0,
        Color32::from_black_alpha(if lit { 170 } else { 140 }),
    );
    painter.rect_stroke(area, 8.0, stroke, egui::StrokeKind::Inside);
    let room = (area.width() - 32.0).max(40.0);
    let line = |text: &str, points: f32| {
        let mut job = egui::text::LayoutJob::simple_singleline(
            text.to_owned(),
            FontId::proportional(points),
            color,
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(room);
        painter.layout_job(job)
    };
    let mut galleys = vec![line(text, 20.0)];
    if let Some(name) = name.filter(|n| !n.is_empty()) {
        galleys.push(line(name, 14.0));
    }
    const GAP: f32 = 6.0;
    let height: f32 =
        galleys.iter().map(|g| g.size().y).sum::<f32>() + GAP * (galleys.len() - 1) as f32;
    let mut y = area.center().y - height / 2.0;
    for galley in galleys {
        let h = galley.size().y;
        let x = area.center().x - galley.size().x / 2.0;
        painter.galley(egui::pos2(x, y), galley, color);
        y += h + GAP;
    }
}

/// The icon of the start screen in points: 112 as in qview, 64 in a low
/// window (both embedded at 100–200 %).
pub(super) fn start_icon_size(rect: Rect) -> f32 {
    if rect.height() >= 340.0 { 112.0 } else { 64.0 }
}
