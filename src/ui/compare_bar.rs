//! The comparison's bar over the picture's top: the two ways of showing
//! A and B, the sides swapped, B's shift in its frames, the zoom, and the
//! end of the comparison. A see-through pill, shown while the pointer
//! moves over the picture (`App::compare_bar`).

use egui::{Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, pos2, vec2};

use super::{icons, style};
use crate::compare::Mode;
use crate::input::Cmd;
use crate::keymap::{ZOOM_STEP, with_key};

/// The bar's height and its buttons' width.
const HEIGHT: f32 = 30.0;
const BUTTON: f32 = 26.0;
const PAD: f32 = 4.0;
/// The space a separator takes.
const SEPARATOR: f32 = 9.0;
/// The widths of the shift's and the zoom's figures, fixed so that the bar
/// does not change its width as they change.
const SHIFT_WIDTH: f32 = 48.0;
const ZOOM_WIDTH: f32 = 50.0;
const ICON: f32 = 6.0;

/// What the bar shows.
pub struct Shown {
    pub mode: Mode,
    pub swapped: bool,
    /// B's shift in its frames, and the same in words for the tooltip.
    pub shift: i64,
    pub shift_text: String,
    /// The zoom in percent; whether the zoom or the pan is not the default.
    pub zoom: i64,
    pub moved: bool,
}

/// The bar with its top centre at `top`, kept within `area`; its rectangle
/// and the commands of its buttons.
pub fn show(ui: &mut Ui, area: Rect, top: Pos2, shown: &Shown) -> (Rect, Vec<Cmd>) {
    let mut out = Vec::new();
    let font = FontId::proportional(12.5);
    let modes: Vec<_> = Mode::ALL
        .iter()
        .map(|m| {
            let galley = ui.painter().layout_no_wrap(
                m.label().to_owned(),
                font.clone(),
                Color32::PLACEHOLDER,
            );
            (*m, galley)
        })
        .collect();
    let modes_width: f32 = modes.iter().map(|(_, g)| g.size().x + 20.0).sum();
    let width = PAD
        + modes_width
        + 2.0
        + BUTTON
        + SEPARATOR
        + BUTTON
        + SHIFT_WIDTH
        + BUTTON
        + SEPARATOR
        + BUTTON
        + ZOOM_WIDTH
        + BUTTON
        + SEPARATOR
        + BUTTON
        + PAD;
    let left = (top.x - width / 2.0)
        .min(area.right() - width)
        .max(area.left());
    let bar = Rect::from_min_size(pos2(left, top.y), vec2(width, HEIGHT));
    // The ground takes the clicks between the buttons: a double click there
    // is not fullscreen, a drag does not move the window.
    ui.interact(bar, ui.id().with("compare_bar"), Sense::click());
    ui.painter()
        .rect_filled(bar, HEIGHT / 2.0, style::bar_bg().gamma_multiply_u8(200));
    let cy = bar.center().y;
    let square = |x: f32, w: f32| {
        Rect::from_min_size(pos2(x, cy - BUTTON / 2.0 + 1.0), vec2(w, BUTTON - 2.0))
    };
    let mut x = bar.left() + PAD;

    // The two ways, as one control: the one shown lit.
    for (mode, galley) in modes {
        let w = galley.size().x + 20.0;
        let rect = square(x, w);
        let on = mode == shown.mode;
        let tip = with_key(mode.label(), Cmd::CycleCompare);
        let response = button(ui, rect, &tip, on);
        let color = if on {
            Color32::WHITE
        } else if response.hovered() {
            style::TEXT_BRIGHT
        } else {
            style::text()
        };
        ui.painter().galley(
            Align2::CENTER_CENTER
                .anchor_size(rect.center(), galley.size())
                .min,
            galley,
            color,
        );
        if response.clicked() && !on {
            out.push(Cmd::SetCompare(mode));
        }
        x += w;
    }
    x += 2.0;
    let tip = with_key(tr!("Swap the sides", "Поменять стороны"), Cmd::SwapCompare);
    let response = button(ui, square(x, BUTTON), &tip, false);
    swap(
        ui.painter(),
        response.rect.center(),
        colour(&response),
        shown.swapped,
    );
    if response.clicked() {
        out.push(Cmd::SwapCompare);
    }
    x += BUTTON;
    x = separator(ui, x, bar);

    // B's shift.
    let tip = with_key(
        tr!("B a frame back", "B на кадр назад"),
        Cmd::CompareShift(-1),
    );
    let response = button(ui, square(x, BUTTON), &tip, false);
    chevron(
        ui.painter(),
        response.rect.center(),
        colour(&response),
        false,
    );
    if response.clicked() {
        out.push(Cmd::CompareShift(-1));
    }
    x += BUTTON;
    let text = if shown.shift == 0 {
        "B 0".to_owned()
    } else {
        format!("B {:+}", shown.shift).replace('-', "−")
    };
    let tip = if shown.shift == 0 {
        shown.shift_text.clone()
    } else {
        format!(
            "{}\n{}",
            shown.shift_text,
            with_key(
                tr!("Click: no shift", "Щелчок: без сдвига"),
                Cmd::CompareShift(0)
            )
        )
    };
    let response = figure(
        ui,
        square(x, SHIFT_WIDTH),
        &text,
        &font,
        &tip,
        shown.shift != 0,
    );
    if response.clicked() && shown.shift != 0 {
        out.push(Cmd::CompareShift(0));
    }
    x += SHIFT_WIDTH;
    let tip = with_key(
        tr!("B a frame forward", "B на кадр вперёд"),
        Cmd::CompareShift(1),
    );
    let response = button(ui, square(x, BUTTON), &tip, false);
    chevron(
        ui.painter(),
        response.rect.center(),
        colour(&response),
        true,
    );
    if response.clicked() {
        out.push(Cmd::CompareShift(1));
    }
    x += BUTTON;
    x = separator(ui, x, bar);

    // The zoom.
    let tip = with_key(tr!("Zoom out", "Уменьшить"), Cmd::Zoom(-ZOOM_STEP));
    let response = button(ui, square(x, BUTTON), &tip, false);
    icons::minus(
        ui.painter(),
        response.rect.center(),
        ICON,
        colour(&response),
    );
    if response.clicked() {
        out.push(Cmd::Zoom(-ZOOM_STEP));
    }
    x += BUTTON;
    let text = tr!(format!("{}%", shown.zoom), format!("{} %", shown.zoom));
    let tip = format!(
        "{}\n{}",
        tr!(
            "The wheel over the picture zooms, the middle button moves it",
            "Колесо над изображением меняет масштаб, средняя кнопка двигает его"
        ),
        with_key(
            tr!("Click: 100 %, centred", "Щелчок: 100 %, по центру"),
            Cmd::ZoomReset
        )
    );
    let response = figure(ui, square(x, ZOOM_WIDTH), &text, &font, &tip, shown.moved);
    if response.clicked() && shown.moved {
        out.push(Cmd::ZoomReset);
    }
    x += ZOOM_WIDTH;
    let tip = with_key(tr!("Zoom in", "Увеличить"), Cmd::Zoom(ZOOM_STEP));
    let response = button(ui, square(x, BUTTON), &tip, false);
    icons::plus(
        ui.painter(),
        response.rect.center(),
        ICON,
        colour(&response),
    );
    if response.clicked() {
        out.push(Cmd::Zoom(ZOOM_STEP));
    }
    x += BUTTON;
    x = separator(ui, x, bar);

    let tip = with_key(
        tr!("End the comparison", "Закончить сравнение"),
        Cmd::EndCompare,
    );
    let response = button(ui, square(x, BUTTON), &tip, false);
    icons::close(
        ui.painter(),
        response.rect.center(),
        ICON * 0.8,
        colour(&response),
    );
    if response.clicked() {
        out.push(Cmd::EndCompare);
    }
    (bar, out)
}

/// A flat button on the bar, `on` lit in the accent colour.
fn button(ui: &mut Ui, rect: Rect, tip: &str, on: bool) -> Response {
    let response = ui.allocate_rect(rect, Sense::CLICK);
    let fill = if on {
        Some(style::accent())
    } else if response.is_pointer_button_down_on() {
        Some(style::pressed())
    } else if response.hovered() {
        Some(style::hover())
    } else {
        None
    };
    if let Some(fill) = fill {
        ui.painter()
            .rect_filled(rect, (HEIGHT - 2.0) / 2.0 - 1.0, fill);
    }
    response.on_hover_text(tip)
}

/// A figure on the bar; clickable (bright when hovered) while `active`.
fn figure(ui: &mut Ui, rect: Rect, text: &str, font: &FontId, tip: &str, active: bool) -> Response {
    let response = if active {
        button(ui, rect, tip, false)
    } else {
        ui.allocate_rect(rect, Sense::hover()).on_hover_text(tip)
    };
    let color = if active && response.hovered() {
        style::TEXT_BRIGHT
    } else if active {
        style::accent()
    } else {
        style::text()
    };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        font.clone(),
        color,
    );
    response
}

fn colour(response: &Response) -> Color32 {
    if response.hovered() {
        style::TEXT_BRIGHT
    } else {
        style::text()
    }
}

/// A thin line between groups; the place after it.
fn separator(ui: &Ui, x: f32, bar: Rect) -> f32 {
    let cx = x + SEPARATOR / 2.0;
    ui.painter().line_segment(
        [pos2(cx, bar.top() + 8.0), pos2(cx, bar.bottom() - 8.0)],
        Stroke::new(1.0, style::line()),
    );
    x + SEPARATOR
}

/// Two arrows, one over the other, pointing apart: the sides swapped.
fn swap(p: &egui::Painter, o: Pos2, c: Color32, swapped: bool) {
    let s = ICON;
    let stroke = Stroke::new(1.5, c);
    // The upper one points the way A goes.
    let dir = if swapped { -1.0 } else { 1.0 };
    for (y, d) in [(-s * 0.45, dir), (s * 0.45, -dir)] {
        let from = pos2(o.x - s * d, o.y + y);
        let to = pos2(o.x + s * d, o.y + y);
        p.line_segment([from, to], stroke);
        p.line_segment([to, to + vec2(-s * 0.5 * d, -s * 0.45)], stroke);
        p.line_segment([to, to + vec2(-s * 0.5 * d, s * 0.45)], stroke);
    }
}

/// A chevron pointing right (`forward`) or left.
fn chevron(p: &egui::Painter, o: Pos2, c: Color32, forward: bool) {
    let s = ICON * 0.75;
    let d = if forward { 1.0 } else { -1.0 };
    let tip = pos2(o.x + s * 0.5 * d, o.y);
    let stroke = Stroke::new(1.6, c);
    p.line_segment([pos2(tip.x - s * d, o.y - s), tip], stroke);
    p.line_segment([pos2(tip.x - s * d, o.y + s), tip], stroke);
}
