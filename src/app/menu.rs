//! The picture's context menu and its submenus.

use super::*;

/// The menu of a right click on the picture.
pub(super) fn context_menu(
    ui: &mut egui::Ui,
    state: &State,
    playlist_visible: bool,
    chapter_marks: bool,
    marks: Marks,
    cmds: &mut Vec<Cmd>,
) {
    // Gathered apart: `item` holds `cmds` until its last use.
    let mut menu_cmds = Vec::new();
    let mut item = |ui: &mut egui::Ui, text: String, cmd: Cmd| {
        let button = egui::Button::new(text).shortcut_text(keymap::shortcut(cmd));
        if ui.add(button).clicked() {
            cmds.push(cmd);
            ui.close();
        }
    };
    if marks.mini {
        item(
            ui,
            tr!("The player's window", "Окно плеера").into(),
            Cmd::ToggleMini,
        );
        ui.separator();
    } else if marks.interface == Interface::Bare {
        // First, where it is found without the interface.
        item(
            ui,
            tr!("Show the interface", "Вернуть интерфейс").into(),
            Cmd::ToggleInterface,
        );
        ui.separator();
    }
    item(ui, tr!("Open…", "Открыть…").into(), Cmd::Open);
    item(
        ui,
        tr!("Open an address…", "Открыть адрес…").into(),
        Cmd::OpenUrl,
    );
    item(
        ui,
        tr!("File information", "Сведения о файле").into(),
        Cmd::ToggleInfo,
    );
    item(
        ui,
        if marks.info_overlay {
            tr!(
                "Hide the information over the picture",
                "Скрыть сведения поверх картинки"
            )
        } else {
            tr!("Information over the picture", "Сведения поверх картинки")
        }
        .into(),
        Cmd::ToggleInfoOverlay,
    );
    item(
        ui,
        tr!("Video, sound and subtitles…", "Видео, звук и субтитры…").into(),
        Cmd::ToggleVideoPanel,
    );
    if video_size(state).is_some() {
        item(
            ui,
            tr!("Save the frame", "Сохранить кадр").into(),
            Cmd::SaveFrame,
        );
        item(
            ui,
            tr!("Save a video map", "Сохранить карту видео").into(),
            Cmd::SaveVideoMap,
        );
    }
    ui.separator();
    let pause = if state.paused || state.idle {
        tr!("Play", "Воспроизвести")
    } else {
        tr!("Pause", "Пауза")
    };
    item(ui, pause.into(), Cmd::TogglePause);
    item(ui, tr!("Stop", "Стоп").into(), Cmd::Stop);
    if !state.idle {
        item(
            ui,
            tr!("Go to time…", "Перейти ко времени…").into(),
            Cmd::GoToTime,
        );
    }
    item(
        ui,
        tr!("Previous file", "Предыдущий файл").into(),
        Cmd::Prev,
    );
    item(ui, tr!("Next file", "Следующий файл").into(), Cmd::Next);
    ui.separator();
    item(
        ui,
        tr!("Fullscreen", "Полный экран").into(),
        Cmd::ToggleFullscreen,
    );
    if !marks.mini {
        item(ui, tr!("Mini player", "Мини-плеер").into(), Cmd::ToggleMini);
    }
    interface_menu(ui, marks.interface, &mut menu_cmds);
    zoom_menu(ui, state, marks.fill, &mut menu_cmds);
    on_top_menu(ui, marks.on_top, &mut menu_cmds);
    aspect_menu(ui, state, marks.aspect, &mut menu_cmds);
    speed_menu(ui, state, &mut menu_cmds);
    repeat_menu(ui, marks.repeat, &mut menu_cmds);
    loop_menu(ui, state, &mut menu_cmds);
    bookmarks_menu(ui, state, &marks.bookmarks, &mut menu_cmds);
    let list = if playlist_visible {
        tr!("Hide the playlist", "Скрыть плейлист")
    } else {
        tr!("Show the playlist", "Показать плейлист")
    };
    item(ui, list.into(), Cmd::TogglePlaylist);
    let window = if marks.playlist_detached {
        tr!("Playlist in the player's window", "Плейлист в окне плеера")
    } else {
        tr!(
            "Playlist in a window of its own",
            "Плейлист в отдельном окне"
        )
    };
    item(ui, window.into(), Cmd::TogglePlaylistWindow);
    let mute = if state.mute {
        tr!("Sound on", "Включить звук")
    } else {
        tr!("Mute", "Без звука")
    };
    item(ui, mute.into(), Cmd::ToggleMute);
    ui.separator();
    crate::ui::tracks::menus(ui, &state.tracks, &mut menu_cmds);
    chapters_menu(ui, state, chapter_marks, &mut menu_cmds);
    ui.separator();
    item(ui, tr!("Settings…", "Настройки…").into(), Cmd::Settings);
    item(ui, tr!("About", "О программе").into(), Cmd::About);
    cmds.extend(menu_cmds);
}

/// The bookmarks: add one here, then each (a click goes there, the cross
/// deletes it; those of the file playing marked), and delete them all.
pub(super) fn bookmarks_menu(
    ui: &mut egui::Ui,
    state: &State,
    list: &[(String, bool, Option<f32>)],
    out: &mut Vec<Cmd>,
) {
    ui.menu_button(tr!("Bookmarks", "Закладки"), |ui| {
        let add = egui::Button::new(tr!("Add a bookmark here", "Добавить закладку здесь"))
            .shortcut_text(keymap::shortcut(Cmd::AddBookmark));
        if ui.add_enabled(!state.idle, add).clicked() {
            out.push(Cmd::AddBookmark);
            ui.close();
        }
        ui.separator();
        if list.is_empty() {
            ui.weak(tr!("No bookmarks yet", "Закладок пока нет"));
        }
        for (i, (label, here, share)) in list.iter().enumerate() {
            ui.horizontal(|ui| {
                let tip = tr!("Delete the bookmark", "Удалить закладку");
                let delete = ui::tool(ui, tip, true, |p, o, c| ui::icons::close(p, o, 4.0, c));
                if delete.clicked() {
                    // The menu stays open: more may go.
                    out.push(Cmd::DeleteBookmark(i));
                }
                let go = ui.add(egui::Button::new(label.as_str()).selected(*here));
                // Where in the file it is, as the playlist's lines show.
                if let Some(share) = share {
                    let rect = go.rect.shrink2(egui::vec2(4.0, 0.0));
                    let track = Rect::from_min_max(
                        egui::pos2(rect.left(), rect.bottom() - 2.0),
                        egui::pos2(rect.right(), rect.bottom() - 0.5),
                    );
                    let painter = ui.painter();
                    painter.rect_filled(track, 0.0, style::text_weak().gamma_multiply(0.3));
                    let done = track.with_max_x(track.left() + track.width() * share);
                    painter.rect_filled(done, 0.0, style::accent().gamma_multiply(0.85));
                }
                if go.clicked() {
                    out.push(Cmd::GoToBookmark(i));
                    ui.close();
                }
            });
        }
        if !list.is_empty() {
            ui.separator();
            if ui
                .button(tr!("Delete all bookmarks", "Удалить все закладки"))
                .clicked()
            {
                out.push(Cmd::ClearBookmarks);
                ui.close();
            }
        }
    });
}

/// What the context menu marks as chosen.
pub(super) struct Marks {
    pub(super) fill: bool,
    /// The interface shown (`8`; only the picture, `7`).
    pub(super) interface: Interface,
    /// The mini player is shown (`0`).
    pub(super) mini: bool,
    pub(super) aspect: crate::adjust::Aspect,
    pub(super) repeat: Repeat,
    pub(super) on_top: OnTop,
    pub(super) playlist_detached: bool,
    pub(super) info_overlay: bool,
    /// The bookmarks' labels, whether each is of the file playing, and
    /// where in its file it is.
    pub(super) bookmarks: Vec<(String, bool, Option<f32>)>,
}

/// The interfaces, the current marked.
pub(super) fn interface_menu(ui: &mut egui::Ui, current: Interface, out: &mut Vec<Cmd>) {
    ui.menu_button(tr!("Interface", "Интерфейс"), |ui| {
        for i in Interface::ALL {
            let key = if i == Interface::Bare {
                keymap::shortcut(Cmd::ToggleInterface)
            } else {
                String::new()
            };
            let button = egui::Button::new(i.label())
                .shortcut_text(key)
                .selected(current == i);
            if ui.add(button).clicked() {
                out.push(Cmd::SetInterface(i));
                ui.close();
            }
        }
    });
}

/// Over the other windows: never, always, while playing; the current
/// marked.
pub(super) fn on_top_menu(ui: &mut egui::Ui, current: OnTop, out: &mut Vec<Cmd>) {
    ui.menu_button(tr!("On top", "Поверх других окон"), |ui| {
        for o in OnTop::ALL {
            if ui.radio(current == o, o.label()).clicked() {
                out.push(Cmd::SetOnTop(o));
                ui.close();
            }
        }
        switches(ui, Cmd::CycleOnTop);
    });
}

/// The line under a submenu that names the key cycling through it, if
/// it has one.
pub(super) fn switches(ui: &mut egui::Ui, cmd: Cmd) {
    let key = keymap::shortcut(cmd);
    if !key.is_empty() {
        ui.separator();
        ui.weak(tr!(
            format!("{key} switches it"),
            format!("{key} переключает")
        ));
    }
}

/// Repeat off, the file or the list, the current marked.
pub(super) fn repeat_menu(ui: &mut egui::Ui, current: Repeat, out: &mut Vec<Cmd>) {
    ui.menu_button(tr!("Repeat", "Повтор"), |ui| {
        for r in Repeat::ALL {
            if ui.radio(current == r, r.label()).clicked() {
                out.push(Cmd::SetRepeat(r));
                ui.close();
            }
        }
        switches(ui, Cmd::CycleRepeat);
    });
}

/// The A-B repeat: its start and its end at the position now, off; the
/// points set, if any.
pub(super) fn loop_menu(ui: &mut egui::Ui, state: &State, out: &mut Vec<Cmd>) {
    ui.menu_button(tr!("A-B repeat", "Повтор A-B"), |ui| {
        let lang = crate::i18n::lang();
        let point = |t: Option<f64>| t.map(|t| crate::format::clock_tenths_in(lang, t));
        let items = [
            (
                Cmd::LoopA,
                tr!("Start here (A)", "Начало здесь (A)"),
                !state.idle,
                point(state.loop_a),
            ),
            (
                Cmd::LoopB,
                tr!("End here (B)", "Конец здесь (B)"),
                !state.idle,
                point(state.loop_b),
            ),
            (
                Cmd::LoopClear,
                tr!("Off", "Выключить"),
                state.loop_a.is_some() || state.loop_b.is_some(),
                None,
            ),
            (
                Cmd::SaveClip,
                tr!("Save the stretch…", "Сохранить фрагмент…"),
                state.loop_a.is_some() && state.loop_b.is_some(),
                None,
            ),
        ];
        for (cmd, text, enabled, at) in items {
            let text = match at {
                Some(at) => format!("{text}: {at}"),
                None => text.to_owned(),
            };
            let button = egui::Button::new(text).shortcut_text(keymap::shortcut(cmd));
            if ui.add_enabled(enabled, button).clicked() {
                out.push(cmd);
                ui.close();
            }
        }
    });
}

/// The playback speed: the steps of `format::SPEEDS`, the current marked,
/// with the keys that change it.
pub(super) fn speed_menu(ui: &mut egui::Ui, state: &State, out: &mut Vec<Cmd>) {
    ui.menu_button(tr!("Speed", "Скорость"), |ui| {
        for speed in crate::format::SPEEDS {
            let text = crate::format::speed_in(crate::i18n::lang(), speed);
            let key = if speed == 1.0 {
                keymap::shortcut(Cmd::Speed(0))
            } else {
                String::new()
            };
            let button = egui::Button::new(text)
                .shortcut_text(key)
                .selected((state.speed - speed).abs() < 1e-9);
            if ui.add(button).clicked() {
                out.push(Cmd::SetSpeed(speed));
                ui.close();
            }
        }
        let faster = keymap::shortcut(Cmd::Speed(1));
        let slower = keymap::shortcut(Cmd::Speed(-1));
        if !faster.is_empty() || !slower.is_empty() {
            ui.separator();
            let line = match (faster.is_empty(), slower.is_empty()) {
                (false, false) => tr!(
                    format!("{faster} faster, {slower} slower"),
                    format!("{faster} быстрее, {slower} медленнее")
                ),
                (false, true) => tr!(format!("{faster} faster"), format!("{faster} быстрее")),
                _ => tr!(format!("{slower} slower"), format!("{slower} медленнее")),
            };
            ui.weak(line);
        }
    });
}

/// The picture's shape: the file's or a standard one, the current marked.
pub(super) fn aspect_menu(
    ui: &mut egui::Ui,
    state: &State,
    current: crate::adjust::Aspect,
    out: &mut Vec<Cmd>,
) {
    ui.menu_button(tr!("Aspect ratio", "Пропорции"), |ui| {
        let video = video_size(state).is_some();
        for a in crate::adjust::Aspect::ALL {
            if ui
                .add_enabled(video, egui::RadioButton::new(current == a, a.label()))
                .clicked()
            {
                out.push(Cmd::SetAspect(a));
                ui.close();
            }
        }
    });
}

/// The picture's size: the window for 50 % to 200 % of the video, the
/// largest window without black bars, the window to the picture shown,
/// and fullscreen without black bars.
pub(super) fn zoom_menu(ui: &mut egui::Ui, state: &State, fill: bool, out: &mut Vec<Cmd>) {
    ui.menu_button(tr!("Zoom", "Масштаб"), |ui| {
        let video = video_size(state).is_some();
        let mut item = |ui: &mut egui::Ui, text: &str, selected: bool, cmd: Cmd| {
            let button = egui::Button::new(text)
                .shortcut_text(keymap::shortcut(cmd))
                .selected(selected);
            if ui.add_enabled(video, button).clicked() {
                out.push(cmd);
                ui.close();
            }
        };
        for scale in [0.5, 1.0, 1.5, 2.0] {
            let text = format!("{:.0}%", scale * 100.0);
            item(ui, &text, false, Cmd::WindowScale(scale));
        }
        ui.separator();
        item(
            ui,
            tr!("Window to the screen", "Окно по экрану"),
            false,
            Cmd::FitScreen,
        );
        item(
            ui,
            tr!("Window to the picture", "Окно по размеру картинки"),
            false,
            Cmd::FitWindow,
        );
        ui.separator();
        item(
            ui,
            tr!(
                "Fullscreen without black bars (crops)",
                "Полный экран без чёрных полос (с обрезкой)"
            ),
            fill,
            Cmd::ToggleFill,
        );
    });
}

/// The chapters' submenu: the marks on the seek bar, then the chapters,
/// the playing one marked; a click goes to one.
pub(super) fn chapters_menu(ui: &mut egui::Ui, state: &State, marks: bool, out: &mut Vec<Cmd>) {
    ui.menu_button(tr!("Chapters", "Главы"), |ui| {
        let mut on = marks;
        if ui
            .checkbox(
                &mut on,
                tr!("Marks on the seek bar", "Метки на полосе перемотки"),
            )
            .clicked()
        {
            out.push(Cmd::ToggleChapterMarks);
        }
        ui.separator();
        if state.idle || state.chapters.is_empty() {
            ui.weak(tr!("No chapters in the file", "В файле нет глав"));
            return;
        }
        let current = state
            .time
            .and_then(|t| crate::chapters::current(&state.chapters, t));
        let long = state.duration.is_some_and(|d| d >= 3600.0);
        egui::ScrollArea::vertical()
            .max_height(420.0)
            .show(ui, |ui| {
                for (i, c) in state.chapters.iter().enumerate() {
                    let text = format!(
                        "{}. {}  ({})",
                        i + 1,
                        c.name(i + 1),
                        crate::format::time(Some(c.time), long)
                    );
                    if ui.radio(current == Some(i), text).clicked() {
                        out.push(Cmd::Chapter(i));
                        ui.close();
                    }
                }
            });
    });
}
