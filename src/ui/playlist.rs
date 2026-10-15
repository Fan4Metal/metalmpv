//! The playlist panel on the right, or in a window of its own: a row of
//! icon buttons (add, remove, sort, clear, save as M3U8; after a line,
//! shuffle and the repeat mode; on the right the button that moves the
//! list into a window of its own or back, and the search; their names in
//! the tooltips), the search field while it is shown, and the list. A
//! click selects an entry (with Ctrl one more, with Shift a range), a
//! double click plays it, the right button opens its menu, a drag moves
//! the selected entries (not while searching or in groups). A thin line
//! under an entry shows how far it was watched (unless turned off in the
//! entries' menu), a tick that it was watched
//! to its end (`resume.rs`). An IPTV list's entries go under their groups,
//! which fold, unless turned off in the entries' and the groups' menu.
//! The list scrolls to the current entry when it changes.
//! Under it, when the playing file has chapters, a section of them that
//! folds: a click goes to a chapter.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use egui::{Align2, CursorIcon, FontId, Painter, Pos2, Rect, Response, Sense, Ui, pos2, vec2};

use super::{icons, style, tool};
use crate::chapters::{self, Chapter};
use crate::input::Cmd;
use crate::logos::Logos;
use crate::playlist::{self, Playlist, Repeat};
use crate::resume::{self, Left, Positions};

const ROW: f32 = 22.0;
/// The narrowest the panel or its window may be, in points, so that the
/// buttons do not run into each other: nine buttons of 26, the line
/// between the groups with its margins (12), the gaps (2 each), a gap
/// before the buttons on the right and the panel's margins (6 a side).
pub const MIN_WIDTH: f32 = 288.0;
/// The height of the chapters' header.
const HEADER: f32 = 26.0;
/// The search field's id: while it has the focus, the keys are its own.
const SEARCH_ID: &str = "playlist_search";

/// The search field has the focus (in the viewport of `ctx`): the player's
/// keys wait.
pub fn typing(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.focused()) == Some(egui::Id::new(SEARCH_ID))
}

/// What the panel keeps between frames, one for all the places it is
/// shown in.
#[derive(Default)]
pub struct ListState {
    /// The search field while it is shown, and what is typed in it.
    search: Option<String>,
    focus_search: bool,
    /// The groups folded, by name; `folds` counts their changes.
    folded: HashSet<String>,
    folds: u64,
    view: Option<View>,
    /// A drag of the selected entries: the slot they would go to.
    drag: Option<Option<usize>>,
    /// The current entry each list (the panel, its window, the fullscreen
    /// one) was last scrolled to.
    scrolled: HashMap<egui::Id, Option<PathBuf>>,
    /// What is kept of the files by `resume::key`, and the revision of the
    /// positions it was made of.
    marks: Option<(u64, HashMap<String, Left>)>,
}

impl ListState {
    /// Show the search field with the focus in it.
    pub fn search(&mut self) {
        self.search.get_or_insert_with(String::new);
        self.focus_search = true;
    }
}

/// The rows of the list: the entries found, under their groups.
struct View {
    /// The playlist's revision, the search, the folds' count and whether
    /// the groups are shown.
    made_of: (u64, String, u64, bool),
    rows: Vec<Row>,
    /// The list has groups (whether shown or not).
    has_groups: bool,
    /// All the entries in the list's order: they can be dragged.
    plain: bool,
}

enum Row {
    Group {
        name: String,
        count: usize,
        folded: bool,
    },
    Entry(usize),
}

impl View {
    /// The list has entries in groups.
    fn has_groups(list: &Playlist) -> bool {
        list.items().iter().any(|p| {
            list.channel(p)
                .and_then(|c| c.group.as_deref())
                .is_some_and(|g| !g.is_empty())
        })
    }

    /// The rows; with `groups`, an IPTV list's entries under their groups.
    fn make(list: &Playlist, query: &str, folded: &HashSet<String>, groups: bool) -> Vec<Row> {
        let query = query.trim().to_lowercase();
        let found = |i: &usize| {
            query.is_empty()
                || list
                    .title(&list.items()[*i])
                    .to_lowercase()
                    .contains(&query)
        };
        let group_of = |i: usize| {
            list.channel(&list.items()[i])
                .and_then(|c| c.group.as_deref())
                .filter(|g| !g.is_empty())
        };
        if !groups || !Self::has_groups(list) {
            return (0..list.len()).filter(found).map(Row::Entry).collect();
        }
        // The groups in the order they first come, each with its entries.
        let mut order = Vec::new();
        let mut members: HashMap<&str, Vec<usize>> = HashMap::new();
        for i in (0..list.len()).filter(found) {
            let group = group_of(i).unwrap_or("");
            members
                .entry(group)
                .or_insert_with(|| {
                    order.push(group);
                    Vec::new()
                })
                .push(i);
        }
        let mut rows = Vec::new();
        for group in order {
            let entries = &members[group];
            // Searching, the groups with entries found are open.
            let folded = query.is_empty() && folded.contains(group);
            rows.push(Row::Group {
                name: group.to_owned(),
                count: entries.len(),
                folded,
            });
            if !folded {
                rows.extend(entries.iter().copied().map(Row::Entry));
            }
        }
        rows
    }
}

/// What the panel shows and changes.
pub struct Panel<'a> {
    pub playlist: &'a mut Playlist,
    pub state: &'a mut ListState,
    /// Where the files were left: how far they were watched.
    pub positions: &'a Positions,
    /// Pictures the channels of an IPTV list, when they are fetched.
    pub logos: Option<&'a mut Logos>,
    pub repeat: Repeat,
    /// An IPTV list's entries under their groups.
    pub groups: bool,
    /// A line under each entry shows how far it was watched.
    pub progress: bool,
    /// The playing file's chapters, position and duration.
    pub chapters: &'a [Chapter],
    pub time: Option<f64>,
    pub duration: Option<f64>,
    /// In a window of its own.
    pub detached: bool,
}

pub fn panel(ui: &mut Ui, panel: Panel<'_>) -> Vec<Cmd> {
    let Panel {
        playlist,
        state,
        positions,
        logos,
        repeat,
        groups,
        progress,
        chapters,
        time,
        duration,
        detached,
    } = panel;
    let mut out = Vec::new();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let s = 5.5;
        let add = tr!("Add files to the list", "Добавить файлы в плейлист");
        if tool(ui, add, true, |p, o, c| icons::plus(p, o, s, c)).clicked() {
            out.push(Cmd::PlaylistAdd);
        }
        let remove = crate::keymap::with_key(
            tr!("Remove the selected entries", "Убрать выбранные записи"),
            Cmd::Delete,
        );
        if tool(ui, &remove, playlist.has_selection(), |p, o, c| {
            icons::minus(p, o, s, c)
        })
        .clicked()
        {
            out.push(Cmd::PlaylistRemoveSelected);
        }
        let sort = tr!("Sort by name", "Упорядочить по имени");
        if tool(ui, sort, playlist.len() > 1, |p, o, c| {
            icons::sort(p, o, s, c)
        })
        .clicked()
        {
            out.push(Cmd::PlaylistSort);
        }
        let clear = tr!("Clear the list", "Очистить плейлист");
        if tool(ui, clear, !playlist.is_empty(), |p, o, c| {
            icons::trash(p, o, 7.0, c)
        })
        .clicked()
        {
            out.push(Cmd::PlaylistClear);
        }
        let save = tr!("Save the list as M3U8…", "Сохранить плейлист в M3U8…");
        if tool(ui, save, !playlist.is_empty(), |p, o, c| {
            icons::save(p, o, s, c)
        })
        .clicked()
        {
            out.push(Cmd::PlaylistSave);
        }
        // The order of play, apart from what acts on the list.
        ui.add_space(3.0);
        ui.separator();
        ui.add_space(3.0);
        let shuffle = tr!("Shuffle the list", "Перемешать плейлист");
        if tool(ui, shuffle, playlist.len() > 1, |p, o, c| {
            icons::shuffle(p, o, s, c)
        })
        .clicked()
        {
            out.push(Cmd::PlaylistShuffle);
        }
        let tip = match repeat {
            Repeat::Off => tr!("Repeat: off", "Повтор: выключен"),
            Repeat::One => tr!("Repeat: the file", "Повтор: файл"),
            Repeat::All => tr!("Repeat: the list", "Повтор: плейлист"),
        };
        let tip = crate::keymap::with_key(tip, Cmd::CycleRepeat);
        if tool(ui, &tip, true, |p, o, c| {
            let c = if repeat == Repeat::Off {
                c
            } else {
                style::accent()
            };
            icons::repeat(p, o, s, c, repeat == Repeat::One)
        })
        .clicked()
        {
            out.push(Cmd::CycleRepeat);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let window = if detached {
                tr!("Back into the player's window", "Вернуть в окно плеера")
            } else {
                tr!("Into a window of its own", "В отдельное окно")
            };
            let window = crate::keymap::with_key(window, Cmd::TogglePlaylistWindow);
            if tool(ui, &window, true, |p, o, c| {
                icons::detach(p, o, s, c, detached)
            })
            .clicked()
            {
                out.push(Cmd::TogglePlaylistWindow);
            }
            let open = state.search.is_some();
            let search = crate::keymap::with_key(
                tr!("Search the list", "Поиск в плейлисте"),
                Cmd::PlaylistSearch,
            );
            if tool(ui, &search, true, |p, o, c| {
                let c = if open { style::accent() } else { c };
                icons::search(p, o, s, c)
            })
            .clicked()
            {
                if open {
                    state.search = None;
                } else {
                    state.search();
                }
            }
        });
    });
    let play_first = search_row(ui, state);
    ui.add_space(2.0);
    // The rows, made again when the list, the search or the folds change.
    let made_of = (
        playlist.revision(),
        state.search.clone().unwrap_or_default(),
        state.folds,
        groups,
    );
    if state.view.as_ref().is_none_or(|v| v.made_of != made_of) {
        let rows = View::make(playlist, &made_of.1, &state.folded, groups);
        let plain = made_of.1.trim().is_empty() && rows.len() == playlist.len();
        let plain = plain && rows.iter().all(|r| matches!(r, Row::Entry(_)));
        state.view = Some(View {
            made_of,
            rows,
            has_groups: View::has_groups(playlist),
            plain,
        });
    }
    if state
        .marks
        .as_ref()
        .is_none_or(|(r, _)| *r != positions.revision())
    {
        state.marks = Some((positions.revision(), positions.by_key()));
    }
    if play_first
        && let Some(i) = state.view.as_ref().and_then(|v| {
            v.rows.iter().find_map(|r| match r {
                Row::Entry(i) => Some(*i),
                Row::Group { .. } => None,
            })
        })
    {
        out.push(Cmd::PlayIndex(i));
    }
    // The chapters take up to half of the height, the files the rest.
    let open_id = ui.id().with("chapters_open");
    let open = ui.data_mut(|d| *d.get_persisted_mut_or(open_id, true));
    let chapters_height = if chapters.is_empty() {
        0.0
    } else if open {
        HEADER + (chapters.len() as f32 * ROW + 4.0).min(ui.available_height() * 0.5)
    } else {
        HEADER
    };
    let files = vec2(
        ui.available_width(),
        ui.available_height() - chapters_height,
    );
    let now = time.zip(duration);
    ui.allocate_ui(files, |ui| {
        files_list(ui, playlist, state, logos, now, progress, &mut out)
    });
    if !chapters.is_empty() {
        if chapters_header(ui, chapters.len(), open).clicked() {
            ui.data_mut(|d| d.insert_persisted(open_id, !open));
        }
        if open {
            chapters_list(ui, chapters, time, &mut out);
        }
    }
    out
}

/// The search field with a cross that closes it, while it is shown. Esc
/// clears it, and closes it when empty; Enter plays the first entry found
/// (true).
fn search_row(ui: &mut Ui, state: &mut ListState) -> bool {
    let focus = std::mem::take(&mut state.focus_search);
    let Some(text) = state.search.as_mut() else {
        return false;
    };
    let mut close = false;
    let mut play = false;
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let edit = egui::TextEdit::singleline(text)
            .id(egui::Id::new(SEARCH_ID))
            .hint_text(tr!("Search the list", "Поиск в плейлисте"))
            .desired_width(ui.available_width() - 28.0)
            // As tall as the cross beside it (a tool button), the text in
            // the middle: lower, the two stood out of line.
            .min_size(vec2(0.0, 24.0))
            .vertical_align(egui::Align::Center);
        let response = ui.add(edit);
        if focus {
            response.request_focus();
        }
        let (esc, enter) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::Enter),
            )
        });
        if response.lost_focus() && esc {
            if text.is_empty() {
                close = true;
            } else {
                text.clear();
                response.request_focus();
            }
        }
        play = response.lost_focus() && enter;
        let cross = tr!("Close the search", "Закрыть поиск");
        if tool(ui, cross, true, |p, o, c| icons::close(p, o, 4.0, c)).clicked() {
            close = true;
        }
    });
    if close {
        state.search = None;
    }
    play
}

/// The files, filling `ui`: an IPTV list's channels by their names, with
/// their logos, under their groups; `now`, the playing file's position and
/// duration; `progress`, how far each entry was watched is shown.
fn files_list(
    ui: &mut Ui,
    playlist: &mut Playlist,
    state: &mut ListState,
    mut logos: Option<&mut Logos>,
    now: Option<(f64, f64)>,
    progress: bool,
    out: &mut Vec<Cmd>,
) {
    let frame = egui::Frame::new()
        .fill(style::LIST_BG)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::same(2));
    let ListState {
        folded,
        folds,
        view,
        drag,
        scrolled,
        marks,
        ..
    } = state;
    let Some(view) = view.as_ref() else {
        return;
    };
    let marks = marks.as_ref().map(|(_, m)| m);
    // The menus' switch of the groups, for a list that has them.
    let groups = view.has_groups.then_some(view.made_of.3);
    frame.show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        if playlist.is_empty() || view.rows.is_empty() {
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.weak(if playlist.is_empty() {
                    tr!(
                        "Drop files here or use Add",
                        "Перетащите файлы сюда или нажмите «Добавить»"
                    )
                } else {
                    tr!("Nothing found", "Ничего не найдено")
                });
            });
            return;
        }
        if !view.plain {
            *drag = None;
        }
        let current = playlist.current();
        let (modifiers, pointer) = ui.input(|i| (i.modifiers, i.pointer.interact_pos()));
        let mut fold = None;
        // All the groups folded (true) or unfolded, from a group's menu.
        let mut fold_all: Option<bool> = None;
        let names = || {
            view.rows.iter().filter_map(|r| match r {
                Row::Group { name, .. } => Some(name),
                Row::Entry(_) => None,
            })
        };
        let all_folded = names().all(|n| folded.contains(n));
        let any_folded = names().any(|n| folded.contains(n));
        // The entries drawn, for the slot a drag would drop them in.
        let mut drawn: Vec<(usize, Rect)> = Vec::new();
        let output = egui::ScrollArea::vertical()
            .id_salt("files")
            .auto_shrink([false, false])
            .show_rows(ui, ROW, view.rows.len(), |ui, range| {
                for r in range {
                    let i = match &view.rows[r] {
                        Row::Group {
                            name,
                            count,
                            folded,
                        } => {
                            let response = group_header(ui, name, *count, *folded);
                            if response.clicked() {
                                fold = Some(name.clone());
                            }
                            response.context_menu(|ui| {
                                let fold = tr!("Fold all groups", "Свернуть все группы");
                                if ui
                                    .add_enabled(!all_folded, egui::Button::new(fold))
                                    .clicked()
                                {
                                    fold_all = Some(true);
                                    ui.close();
                                }
                                let unfold = tr!("Unfold all groups", "Развернуть все группы");
                                if ui
                                    .add_enabled(any_folded, egui::Button::new(unfold))
                                    .clicked()
                                {
                                    fold_all = Some(false);
                                    ui.close();
                                }
                                ui.separator();
                                groups_item(ui, groups, out);
                            });
                            continue;
                        }
                        Row::Entry(i) => *i,
                    };
                    let path = playlist.items()[i].clone();
                    let sense = if view.plain {
                        Sense::click_and_drag()
                    } else {
                        Sense::click()
                    };
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), ROW), sense);
                    drawn.push((i, rect));
                    let left = marks.and_then(|m| m.get(&resume::key(&path)));
                    let watched_part = if !progress {
                        None
                    } else if current == Some(i) {
                        now.filter(|(_, d)| *d >= resume::MIN_DURATION)
                            .map(|(t, d)| (t / d).clamp(0.0, 1.0) as f32)
                    } else {
                        left.and_then(Left::progress)
                    };
                    let entry = Entry {
                        number: i + 1,
                        name: playlist.title(&path),
                        current: current == Some(i),
                        selected: playlist.is_selected(i),
                        hovered: response.hovered(),
                        watched: left.is_some_and(|l| l.watched),
                        progress: watched_part,
                    };
                    let channel = playlist.channel(&path);
                    let logo = channel
                        .and_then(|c| c.logo.as_deref())
                        .zip(logos.as_deref_mut())
                        .map(|(url, logos)| logos.get(ui.ctx(), url));
                    paint_entry(ui, rect, &entry, logo);
                    let tip = match channel.and_then(|c| c.group.as_deref()) {
                        Some(group) => format!("{group}\n{}", path.display()),
                        None => path.display().to_string(),
                    };
                    let response = response.on_hover_text(tip);
                    if response.secondary_clicked() && !entry.selected {
                        playlist.select(i);
                    }
                    if crate::ui::double_clicked(&response) {
                        out.push(Cmd::PlayIndex(i));
                    } else if response.clicked() {
                        if modifiers.command {
                            playlist.toggle_selected(i);
                        } else if modifiers.shift {
                            playlist.select_range(i);
                        } else {
                            playlist.select(i);
                        }
                    }
                    if view.plain && response.drag_started() {
                        if !playlist.is_selected(i) {
                            playlist.select(i);
                        }
                        *drag = Some(None);
                    }
                    response.context_menu(|ui| {
                        entry_menu(ui, playlist, i, out);
                        ui.separator();
                        let mut on = progress;
                        let text = tr!("Show how far files went", "Показывать полосы просмотра");
                        if ui.checkbox(&mut on, text).clicked() {
                            out.push(Cmd::TogglePlaylistProgress);
                            ui.close();
                        }
                        groups_item(ui, groups, out);
                    });
                }
                if let (Some(slot), Some(pos)) = (drag.as_mut(), pointer) {
                    *slot = drop_slot(ui, &drawn, pos);
                }
            });
        if let Some(name) = fold {
            if !folded.remove(&name) {
                folded.insert(name);
            }
            *folds += 1;
        }
        match fold_all {
            Some(true) => {
                let all: Vec<String> = names().cloned().collect();
                folded.extend(all);
                *folds += 1;
            }
            Some(false) => {
                folded.clear();
                *folds += 1;
            }
            None => {}
        }
        // Let go: dropped where the line was (taken in any case).
        if drag.is_some()
            && !ui.input(|i| i.pointer.primary_down())
            && let Some(Some(slot)) = drag.take()
        {
            out.push(Cmd::PlaylistMove(slot));
        }
        // The current entry changed (or this list is new): scrolled into
        // sight when it is out of it.
        let current_path = current.map(|i| playlist.items()[i].clone());
        if scrolled.get(&output.id) != Some(&current_path) {
            let row = current.and_then(|c| {
                view.rows
                    .iter()
                    .position(|r| matches!(r, Row::Entry(i) if *i == c))
            });
            if let Some(row) = row {
                let step = ROW + ui.spacing().item_spacing.y;
                let top = row as f32 * step;
                let height = output.inner_rect.height();
                let mut scroll = output.state;
                if top < scroll.offset.y || top + ROW > scroll.offset.y + height {
                    let most = (output.content_size.y - height).max(0.0);
                    scroll.offset.y = (top - (height - ROW) / 2.0).clamp(0.0, most);
                    scroll.store(ui.ctx(), output.id);
                    ui.ctx().request_repaint();
                }
            }
            scrolled.insert(output.id, current_path);
        }
    });
}

/// What a row shows of an entry.
struct Entry {
    number: usize,
    name: String,
    current: bool,
    selected: bool,
    hovered: bool,
    watched: bool,
    /// How far it was watched, 0 to 1.
    progress: Option<f32>,
}

/// An entry's row: its number, the channel's logo (`Some` while one is
/// wanted, `Some(None)` until it comes), the name, a tick when watched and
/// a thin line under it for how far.
fn paint_entry(ui: &Ui, rect: Rect, entry: &Entry, logo: Option<Option<&egui::TextureHandle>>) {
    if entry.selected {
        ui.painter().rect_filled(rect, 2.0, style::selected_bg());
    } else if entry.hovered {
        ui.painter().rect_filled(rect, 2.0, style::HOVER);
    }
    let color = if entry.current {
        style::current()
    } else {
        style::TEXT
    };
    let painter = ui.painter_at(rect.shrink2(vec2(4.0, 0.0)));
    let font = FontId::proportional(13.0);
    let n = painter.text(
        rect.left_center() + vec2(6.0, 0.0),
        Align2::LEFT_CENTER,
        format!("{}.", entry.number),
        font.clone(),
        style::TEXT_WEAK,
    );
    let mut text_left = n.right() + 6.0;
    if let Some(texture) = logo {
        // A square the row's height, the logo fitted in it.
        let side = ROW - 4.0;
        let square = Rect::from_min_size(
            pos2(text_left, rect.center().y - side / 2.0),
            vec2(side, side),
        );
        if let Some(texture) = texture {
            let size = texture.size_vec2();
            let scale = (side / size.x).min(side / size.y);
            let shown = Rect::from_center_size(square.center(), size * scale);
            painter.image(
                texture.id(),
                shown,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        text_left = square.right() + 6.0;
    }
    let mut text_right = rect.right() - 6.0;
    if entry.watched {
        icons::check(
            &painter,
            pos2(rect.right() - 13.0, rect.center().y),
            3.5,
            style::TEXT_WEAK,
        );
        text_right -= 16.0;
    }
    let galley = ui.fonts_mut(|f| {
        let mut job = egui::text::LayoutJob::simple_singleline(entry.name.clone(), font, color);
        job.wrap = egui::text::TextWrapping::truncate_at_width(text_right - text_left);
        f.layout_job(job)
    });
    painter.galley(
        pos2(text_left, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    if let Some(progress) = entry.progress.filter(|p| *p > 0.005) {
        let track = Rect::from_min_max(
            pos2(text_left, rect.bottom() - 2.5),
            pos2(rect.right() - 6.0, rect.bottom() - 1.0),
        );
        painter.rect_filled(track, 0.0, style::TEXT_WEAK.gamma_multiply(0.3));
        let done = track.with_max_x(track.left() + track.width() * progress);
        painter.rect_filled(done, 0.0, style::accent().gamma_multiply(0.85));
    }
}

/// The slot the dragged entries would go to under `pos` (0 before the
/// first entry), shown by a line; near the list's edges it scrolls.
fn drop_slot(ui: &Ui, drawn: &[(usize, Rect)], pos: Pos2) -> Option<usize> {
    let (&(first, first_rect), &(last, last_rect)) = (drawn.first()?, drawn.last()?);
    let slot = if pos.y < first_rect.top() {
        first
    } else {
        drawn
            .iter()
            .find(|(_, r)| pos.y < r.bottom())
            .map_or(
                last + 1,
                |(i, r)| if pos.y < r.center().y { *i } else { i + 1 },
            )
    };
    let y = drawn
        .iter()
        .find(|(i, _)| *i == slot)
        .map_or(last_rect.bottom() + 1.0, |(_, r)| r.top() - 1.0);
    ui.painter().hline(
        first_rect.x_range(),
        y,
        egui::Stroke::new(2.0, style::accent()),
    );
    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    let clip = ui.clip_rect();
    if pos.y < clip.top() + ROW {
        ui.scroll_with_delta(vec2(0.0, 6.0));
    } else if pos.y > clip.bottom() - ROW {
        ui.scroll_with_delta(vec2(0.0, -6.0));
    }
    ui.ctx().request_repaint();
    Some(slot)
}

/// The menu of entry `index` (selected with the right button): play it,
/// show it in its folder, copy the paths of those selected, select them
/// all, remove those selected.
fn entry_menu(ui: &mut Ui, list: &mut Playlist, index: usize, out: &mut Vec<Cmd>) {
    let path = list.items()[index].clone();
    let address = playlist::is_url(&path);
    if ui.button(tr!("Play", "Воспроизвести")).clicked() {
        out.push(Cmd::PlayIndex(index));
        ui.close();
    }
    let folder = egui::Button::new(tr!("Show in its folder", "Показать в папке"));
    if ui.add_enabled(!address, folder).clicked() {
        out.push(Cmd::ShowEntryInFolder(index));
        ui.close();
    }
    let selection = list.selection();
    let many = selection.len();
    let copy = if many > 1 {
        tr!(
            format!("Copy the paths ({many})"),
            format!("Копировать пути ({many})")
        )
    } else if address {
        tr!("Copy the address", "Копировать адрес").to_owned()
    } else {
        tr!("Copy the path", "Копировать путь").to_owned()
    };
    if ui.button(copy).clicked() {
        let text: Vec<String> = selection
            .iter()
            .map(|i| list.items()[*i].display().to_string())
            .collect();
        ui.ctx().copy_text(text.join("\r\n"));
        ui.close();
    }
    ui.separator();
    if ui.button(tr!("Select all", "Выделить все")).clicked() {
        list.select_all();
        ui.close();
    }
    let remove = if many > 1 {
        tr!(
            format!("Remove from the list ({many})"),
            format!("Убрать из плейлиста ({many})")
        )
    } else {
        tr!("Remove from the list", "Убрать из плейлиста").to_owned()
    };
    let remove = egui::Button::new(remove).shortcut_text(crate::keymap::shortcut(Cmd::Delete));
    if ui.add(remove).clicked() {
        out.push(Cmd::PlaylistRemoveSelected);
        ui.close();
    }
}

/// The switch of the groups in a menu, while the list has them (`groups`,
/// whether they are shown).
fn groups_item(ui: &mut Ui, groups: Option<bool>, out: &mut Vec<Cmd>) {
    let Some(shown) = groups else {
        return;
    };
    let mut on = shown;
    if ui
        .checkbox(&mut on, tr!("Show the groups", "Показывать группы"))
        .clicked()
    {
        out.push(Cmd::TogglePlaylistGroups);
        ui.close();
    }
}

/// An arrow, down while `open`: a section that folds.
fn fold_arrow(painter: &Painter, o: Pos2, open: bool) {
    let s = 3.5;
    let arrow = if open {
        vec![
            o + vec2(-s, -s * 0.6),
            o + vec2(s, -s * 0.6),
            o + vec2(0.0, s * 0.7),
        ]
    } else {
        vec![
            o + vec2(-s * 0.6, -s),
            o + vec2(s * 0.7, 0.0),
            o + vec2(-s * 0.6, s),
        ]
    };
    painter.add(egui::Shape::convex_polygon(
        arrow,
        style::TEXT_WEAK,
        egui::Stroke::NONE,
    ));
}

/// The header of a group of an IPTV list: an arrow, its name and how many
/// entries it has (found); a click folds or unfolds it.
fn group_header(ui: &mut Ui, name: &str, count: usize, folded: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::CLICK);
    if response.hovered() {
        ui.painter().rect_filled(rect, 2.0, style::HOVER);
    }
    let painter = ui.painter_at(rect);
    fold_arrow(&painter, pos2(rect.left() + 10.0, rect.center().y), !folded);
    let n = painter.text(
        pos2(rect.right() - 6.0, rect.center().y),
        Align2::RIGHT_CENTER,
        count.to_string(),
        FontId::proportional(12.0),
        style::TEXT_WEAK,
    );
    let name = if name.is_empty() {
        tr!("No group", "Без группы")
    } else {
        name
    };
    let left = rect.left() + 20.0;
    let galley = ui.fonts_mut(|f| {
        let mut job = egui::text::LayoutJob::simple_singleline(
            name.to_owned(),
            FontId::proportional(13.0),
            style::TEXT_BRIGHT,
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(n.left() - 8.0 - left);
        f.layout_job(job)
    });
    painter.galley(
        pos2(left, rect.center().y - galley.size().y / 2.0),
        galley,
        style::TEXT_BRIGHT,
    );
    response
}

/// The header of the chapters: an arrow (down while open), "Chapters",
/// their number; a click folds or unfolds them.
fn chapters_header(ui: &mut Ui, count: usize, open: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), HEADER), Sense::CLICK);
    if response.hovered() {
        ui.painter()
            .rect_filled(rect.shrink2(vec2(0.0, 2.0)), 3.0, style::HOVER);
    }
    let painter = ui.painter();
    fold_arrow(painter, pos2(rect.left() + 12.0, rect.center().y), open);
    painter.text(
        pos2(rect.left() + 24.0, rect.center().y),
        Align2::LEFT_CENTER,
        tr!("Chapters", "Главы"),
        FontId::proportional(13.0),
        style::TEXT,
    );
    painter.text(
        pos2(rect.right() - 6.0, rect.center().y),
        Align2::RIGHT_CENTER,
        count.to_string(),
        FontId::proportional(12.0),
        style::TEXT_WEAK,
    );
    response.on_hover_text(if open {
        tr!("Hide the chapters", "Свернуть главы")
    } else {
        tr!("Show the chapters", "Развернуть главы")
    })
}

/// The chapters, filling `ui`: the number, the name, the start on the
/// right; the playing one in the playing entry's colour.
fn chapters_list(ui: &mut Ui, chapters: &[Chapter], time: Option<f64>, out: &mut Vec<Cmd>) {
    let current = time.and_then(|t| chapters::current(chapters, t));
    let long = chapters.last().is_some_and(|c| c.time >= 3600.0);
    let frame = egui::Frame::new()
        .fill(style::LIST_BG)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::same(2));
    frame.show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        egui::ScrollArea::vertical()
            .id_salt("chapters")
            .auto_shrink([false, false])
            .show_rows(ui, ROW, chapters.len(), |ui, range| {
                for i in range {
                    let chapter = &chapters[i];
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
                    if response.hovered() {
                        ui.painter().rect_filled(rect, 2.0, style::HOVER);
                    }
                    let color = if current == Some(i) {
                        style::current()
                    } else {
                        style::TEXT
                    };
                    let painter = ui.painter_at(rect.shrink2(vec2(4.0, 0.0)));
                    let font = FontId::proportional(13.0);
                    let n = painter.text(
                        rect.left_center() + vec2(6.0, 0.0),
                        Align2::LEFT_CENTER,
                        format!("{}.", i + 1),
                        font.clone(),
                        style::TEXT_WEAK,
                    );
                    let start = painter.text(
                        rect.right_center() - vec2(6.0, 0.0),
                        Align2::RIGHT_CENTER,
                        crate::format::time(Some(chapter.time), long),
                        FontId::monospace(11.5),
                        style::TEXT_WEAK,
                    );
                    let name = chapter.name(i + 1);
                    let text_left = n.right() + 6.0;
                    let galley = ui.fonts_mut(|f| {
                        let mut job =
                            egui::text::LayoutJob::simple_singleline(name.clone(), font, color);
                        job.wrap = egui::text::TextWrapping::truncate_at_width(
                            start.left() - 8.0 - text_left,
                        );
                        f.layout_job(job)
                    });
                    painter.galley(
                        pos2(text_left, rect.center().y - galley.size().y / 2.0),
                        galley,
                        color,
                    );
                    if response.on_hover_text(name).clicked() {
                        out.push(Cmd::Chapter(i));
                    }
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playlist::Channel;

    fn entries(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|r| match r {
                Row::Group { name, count, .. } => format!("[{name} {count}]"),
                Row::Entry(i) => i.to_string(),
            })
            .collect()
    }

    #[test]
    fn rows_of_groups_and_of_a_search() {
        let mut list = Playlist::default();
        let urls: Vec<PathBuf> = (0..5).map(|i| format!("https://tv/{i}").into()).collect();
        list.replace(urls.clone());
        let channel = |title: &str, group: Option<&str>| Channel {
            title: Some(title.into()),
            group: group.map(Into::into),
            ..Channel::default()
        };
        // No group yet: the entries as they are.
        assert_eq!(
            entries(&View::make(&list, "", &HashSet::new(), true)),
            ["0", "1", "2", "3", "4"]
        );
        list.describe(HashMap::from([
            (urls[0].clone(), channel("News One", Some("News"))),
            (urls[1].clone(), channel("Kino", Some("Films"))),
            (urls[2].clone(), channel("News Two", Some("News"))),
            (urls[3].clone(), channel("Other", None)),
        ]));
        // The groups in the order they first come; one without a group.
        assert_eq!(
            entries(&View::make(&list, "", &HashSet::new(), true)),
            ["[News 2]", "0", "2", "[Films 1]", "1", "[ 2]", "3", "4"]
        );
        // Turned off: the entries as they are.
        assert_eq!(
            entries(&View::make(&list, "", &HashSet::new(), false)),
            ["0", "1", "2", "3", "4"]
        );
        let folded = HashSet::from(["News".to_owned()]);
        assert_eq!(
            entries(&View::make(&list, "", &folded, true)),
            ["[News 2]", "[Films 1]", "1", "[ 2]", "3", "4"]
        );
        // Searching: the groups found, open, with what was found.
        assert_eq!(
            entries(&View::make(&list, " news ", &folded, true)),
            ["[News 2]", "0", "2"]
        );
        assert!(View::make(&list, "nothing", &folded, true).is_empty());
    }
}
