//! The subtitles' timeline on the panel's Subtitles tab: a stretch of time
//! round the position (the line in the middle), the lines of a subtitle
//! file of its own as blocks moved by the delay. Dragging the blocks sets
//! the delay, a block's start held on the position line for a moment
//! (`SNAP`); ‹ and › put the line before or after the one on screen at the
//! position (mpv's `sub-step`, for any subtitles), − and + widen or narrow
//! the stretch. The way to use it: pause where a line is spoken, then put
//! its block on the position line.

use egui::{Color32, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::{icons, style, tool};
use crate::input::Cmd;
use crate::sub_lines::Line;

/// The stretches of time shown, in seconds; the third at first.
const SPANS: [f64; 6] = [10.0, 20.0, 40.0, 80.0, 160.0, 320.0];
const START_SPAN: usize = 2;
/// A block's start this near the position line (in points) is put on it
/// while dragging.
const SNAP: f32 = 5.0;
const HEIGHT: f32 = 24.0;

/// What the timeline shows.
#[derive(Clone, Copy, Debug)]
pub struct Timeline<'a> {
    /// The lines of the subtitle file shown, when it is one of its own.
    pub lines: Option<&'a [Line]>,
    /// The position.
    pub time: Option<f64>,
    pub delay: f64,
    /// Subtitles are shown (‹ and › have lines to move).
    pub subs_on: bool,
    /// They are of a file of their own (whose lines may not be read).
    pub subs_external: bool,
}

/// The row: ‹, −, the strip, +, ›.
pub fn show(ui: &mut Ui, now: Timeline<'_>, out: &mut Vec<Cmd>) {
    let span_id = egui::Id::new("sub_timeline_span");
    let mut span = ui.data(|d| d.get_temp::<usize>(span_id).unwrap_or(START_SPAN));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let s = 4.5;
        let back = crate::keymap::with_key(
            tr!(
                "The line before the one on screen, now",
                "Предыдущую реплику — на текущий момент"
            ),
            Cmd::SubStep(-1),
        );
        if tool(ui, &back, now.subs_on, |p, o, c| chevron(p, o, s, c, false)).clicked() {
            out.push(Cmd::SubStep(-1));
        }
        let wider = tr!("A longer stretch of time", "Показать больше времени");
        if tool(ui, wider, span + 1 < SPANS.len(), |p, o, c| {
            icons::minus(p, o, s, c)
        })
        .clicked()
        {
            span += 1;
        }
        // The strip takes what the buttons leave.
        let width = ui.available_width() - 2.0 * 26.0 - 2.0 * ui.spacing().item_spacing.x;
        let (strip, response) =
            ui.allocate_exact_size(vec2(width.max(40.0), HEIGHT), Sense::click_and_drag());
        strip_contents(ui, strip, &response, now, SPANS[span], out);
        let narrower = tr!("A shorter stretch of time", "Показать меньше времени");
        if tool(ui, narrower, span > 0, |p, o, c| icons::plus(p, o, s, c)).clicked() {
            span -= 1;
        }
        let next = crate::keymap::with_key(
            tr!(
                "The line after the one on screen, now",
                "Следующую реплику — на текущий момент"
            ),
            Cmd::SubStep(1),
        );
        if tool(ui, &next, now.subs_on, |p, o, c| chevron(p, o, s, c, true)).clicked() {
            out.push(Cmd::SubStep(1));
        }
    });
    ui.data_mut(|d| d.insert_temp(span_id, span));
}

/// The strip: the blocks round the position, `span` seconds in all, and the
/// drag that moves them.
fn strip_contents(
    ui: &Ui,
    strip: Rect,
    response: &egui::Response,
    now: Timeline<'_>,
    span: f64,
    out: &mut Vec<Cmd>,
) {
    let painter = ui.painter_at(strip);
    painter.rect_filled(strip, 3.0, style::LIST_BG);
    let note = |text: &str| {
        painter.text(
            strip.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(11.5),
            style::TEXT_WEAK,
        );
    };
    let (Some(lines), Some(time)) = (now.lines, now.time) else {
        note(if !now.subs_on {
            tr!("No subtitles", "Нет субтитров")
        } else if now.subs_external {
            // Not read (yet): VobSub, PGS, SAMI…
            tr!(
                "The lines of this file are not shown",
                "Реплики этого файла не показываются"
            )
        } else {
            tr!(
                "The timeline is for subtitle files",
                "Шкала — для субтитров из файла"
            )
        });
        return;
    };
    let per_second = f64::from(strip.width()) / span;
    let centre = strip.center().x;
    let x_of = |t: f64| centre + ((t - time) * per_second) as f32;
    // The drag: the delay at its start plus the way the pointer went from
    // where it was pressed (egui's drag starts some points later, and the
    // blocks lagged behind it by those), so that the hold on the position
    // line can be left by going on.
    let drag_id = response.id.with("drag");
    if response.drag_started() {
        let origin = ui.input(|i| i.pointer.press_origin()).map_or(0.0, |p| p.x);
        ui.data_mut(|d| d.insert_temp(drag_id, (now.delay, origin)));
    }
    let mut delay = now.delay;
    if response.dragged()
        && let Some((start, origin)) = ui.data(|d| d.get_temp::<(f64, f32)>(drag_id))
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let raw = start + f64::from(pointer.x - origin) / per_second;
        // The start nearest to the position line, held on it.
        let nearest = nearest_start(lines, time - raw);
        delay = match nearest {
            Some(t) if ((t + raw - time) * per_second).abs() < f64::from(SNAP) => time - t,
            _ => raw,
        };
        if (delay - now.delay).abs() > 0.0005 {
            out.push(Cmd::SetSubDelay(delay));
        }
    }
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(if response.dragged() {
            egui::CursorIcon::Grabbing
        } else {
            egui::CursorIcon::Grab
        });
    }
    // The lines that show in the stretch (a line longer than a minute that
    // started before it is let go).
    let (from, to) = (time - delay - span / 2.0, time - delay + span / 2.0);
    let first = lines.partition_point(|l| l.start < from - 60.0);
    let blocks = strip.shrink2(vec2(0.0, 5.0));
    for line in lines[first..].iter().take_while(|l| l.start <= to) {
        if line.end < from {
            continue;
        }
        let (start, end) = (line.start + delay, line.end + delay);
        let rect = Rect::from_x_y_ranges(
            x_of(start)..=x_of(end).max(x_of(start) + 2.0),
            blocks.y_range(),
        );
        let color = if start <= time && time < end {
            style::accent().gamma_multiply(0.8)
        } else if end <= time {
            style::accent().gamma_multiply(0.35)
        } else {
            style::TEXT_WEAK.gamma_multiply(0.35)
        };
        painter.rect_filled(rect, 1.0, color);
    }
    // The position.
    painter.line_segment(
        [pos2(centre, strip.top()), pos2(centre, strip.bottom())],
        Stroke::new(2.0, style::accent()),
    );
    response.clone().on_hover_text(tr!(
        "Drag the blocks to move the subtitles; a line's start stops for a moment at the position. The way to match them: pause where a line is spoken and put its block on the position",
        "Перетаскивайте блоки, чтобы сдвинуть субтитры; начало реплики ненадолго задерживается на текущем моменте. Как подогнать: пауза там, где звучит реплика, и её блок — на линию текущего момента"
    ));
}

/// The line start nearest to `t` (both in the file's own time).
fn nearest_start(lines: &[Line], t: f64) -> Option<f64> {
    let i = lines.partition_point(|l| l.start < t);
    [i.checked_sub(1), Some(i)]
        .into_iter()
        .flatten()
        .filter_map(|i| lines.get(i))
        .map(|l| l.start)
        .min_by(|a, b| (a - t).abs().total_cmp(&(b - t).abs()))
}

/// ‹ or ›.
fn chevron(p: &egui::Painter, o: egui::Pos2, s: f32, c: Color32, right: bool) {
    let x = if right { 0.4 } else { -0.4 };
    p.add(egui::Shape::line(
        vec![
            o + vec2(-x, -1.0) * s,
            o + vec2(x, 0.0) * s,
            o + vec2(-x, 1.0) * s,
        ],
        Stroke::new(1.6, c),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_start() {
        let lines: Vec<Line> = [1.0, 5.0, 9.0]
            .iter()
            .map(|&start| Line {
                start,
                end: start + 1.0,
            })
            .collect();
        assert_eq!(nearest_start(&lines, 6.0), Some(5.0));
        assert_eq!(nearest_start(&lines, 7.5), Some(9.0));
        assert_eq!(nearest_start(&lines, -3.0), Some(1.0));
        assert_eq!(nearest_start(&lines, 99.0), Some(9.0));
        assert_eq!(nearest_start(&[], 1.0), None);
    }
}
