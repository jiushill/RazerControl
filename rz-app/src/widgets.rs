//! 自绘交互控件：滑杆 / 开关 / 分段选择 / 环形电量 / 按钮 / 导航 / 图标。

use crate::theme::*;
use egui::{
    Align2, Color32, CursorIcon, FontId, Id, Pos2, Rect, Response, Rounding, Sense, Shape, Stroke,
    Ui, Vec2,
};

// ---------------- 图标 ----------------
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Dashboard,
    Dpi,
    Perf,
    Buttons,
    Macro,
    Light,
    Settings,
    Min,
    Max,
    Close,
    Bolt,
    Mouse,
    Reset,
    Info,
    Chevron,
    Calib,
}

pub fn icon(p: &egui::Painter, rect: Rect, ic: Icon, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let w = (s * 0.09).max(1.3);
    let st = Stroke::new(w, color);
    match ic {
        Icon::Macro => {
            // 圆环 + 播放三角 + 左上录制点
            p.circle_stroke(c, s * 0.40, st);
            let r = s * 0.17;
            let tri = vec![
                Pos2::new(c.x - r * 0.7, c.y - r),
                Pos2::new(c.x - r * 0.7, c.y + r),
                Pos2::new(c.x + r, c.y),
            ];
            p.add(Shape::convex_polygon(tri, color, Stroke::NONE));
            p.circle_filled(c + Vec2::new(-s * 0.34, -s * 0.34), s * 0.075, color);
        }
        Icon::Dashboard => {
            let g = s * 0.10;
            let q = (s - g * 3.0) / 2.0;
            for (i, j) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let min = Pos2::new(
                    c.x - s / 2.0 + i as f32 * (q + g),
                    c.y - s / 2.0 + j as f32 * (q + g),
                );
                p.rect_filled(
                    Rect::from_min_size(min, Vec2::splat(q)),
                    Rounding::same(s * 0.08),
                    color,
                );
            }
        }
        Icon::Dpi => {
            p.circle_stroke(c, s * 0.40, st);
            p.circle_filled(c, s * 0.08, color);
            for d in [
                Vec2::new(0.0, -1.0),
                Vec2::new(0.0, 1.0),
                Vec2::new(-1.0, 0.0),
                Vec2::new(1.0, 0.0),
            ] {
                p.line_segment([c + d * s * 0.46, c + d * s * 0.60], st);
            }
        }
        Icon::Perf => {
            let pts = arc_points(c + Vec2::new(0.0, s * 0.18), s * 0.40, -2.6, -0.55, 24);
            p.add(Shape::line(pts, st));
            let ang = -1.1f32;
            p.line_segment(
                [c + Vec2::new(0.0, s * 0.18), c + Vec2::new(0.0, s * 0.18) + Vec2::new(ang.cos(), ang.sin()) * s * 0.34],
                Stroke::new(w * 1.3, color),
            );
            p.circle_filled(c + Vec2::new(0.0, s * 0.18), s * 0.07, color);
        }
        Icon::Buttons | Icon::Mouse => {
            let r = Rect::from_center_size(c, Vec2::new(s * 0.56, s * 0.84));
            p.rect_stroke(r, Rounding::same(s * 0.26), st);
            p.line_segment(
                [Pos2::new(c.x, r.top() + s * 0.06), Pos2::new(c.x, r.top() + s * 0.34)],
                st,
            );
            p.rect_filled(
                Rect::from_center_size(
                    Pos2::new(c.x, r.top() + s * 0.20),
                    Vec2::new(s * 0.09, s * 0.18),
                ),
                Rounding::same(s * 0.05),
                color,
            );
        }
        Icon::Light => {
            p.circle_stroke(c + Vec2::new(0.0, -s * 0.10), s * 0.26, st);
            for a in [-0.9f32, 0.0, 0.9] {
                let dir = Vec2::new(a.sin(), -a.cos());
                p.line_segment(
                    [
                        c + Vec2::new(0.0, -s * 0.10) + dir * s * 0.34,
                        c + Vec2::new(0.0, -s * 0.10) + dir * s * 0.46,
                    ],
                    st,
                );
            }
            p.line_segment(
                [c + Vec2::new(-s * 0.10, s * 0.28), c + Vec2::new(s * 0.10, s * 0.28)],
                st,
            );
        }
        Icon::Settings => {
            p.circle_stroke(c, s * 0.22, st);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let d = Vec2::new(a.cos(), a.sin());
                p.line_segment([c + d * s * 0.30, c + d * s * 0.42], st);
            }
        }
        Icon::Min => {
            p.line_segment(
                [c + Vec2::new(-s * 0.30, 0.0), c + Vec2::new(s * 0.30, 0.0)],
                st,
            );
        }
        Icon::Max => {
            p.rect_stroke(
                Rect::from_center_size(c, Vec2::splat(s * 0.56)),
                Rounding::same(s * 0.06),
                st,
            );
        }
        Icon::Close => {
            let d = s * 0.26;
            p.line_segment([c + Vec2::new(-d, -d), c + Vec2::new(d, d)], st);
            p.line_segment([c + Vec2::new(d, -d), c + Vec2::new(-d, d)], st);
        }
        Icon::Bolt => {
            let pts = vec![
                c + Vec2::new(s * 0.06, -s * 0.42),
                c + Vec2::new(-s * 0.24, s * 0.04),
                c + Vec2::new(-s * 0.02, s * 0.04),
                c + Vec2::new(-s * 0.08, s * 0.42),
                c + Vec2::new(s * 0.26, -s * 0.06),
                c + Vec2::new(s * 0.03, -s * 0.06),
            ];
            p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Icon::Reset => {
            let pts = arc_points(c, s * 0.34, -1.2, 4.2, 22);
            p.add(Shape::line(pts, st));
            let tip = c + Vec2::new(4.2f32.cos(), 4.2f32.sin()) * s * 0.34;
            let dir = Vec2::new(4.2f32.cos(), 4.2f32.sin()) * s * 0.10;
            let side = Vec2::new(-4.2f32.sin(), 4.2f32.cos()) * s * 0.10;
            p.add(Shape::convex_polygon(
                vec![tip + dir, tip - side, tip + side],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Info => {
            p.circle_stroke(c, s * 0.40, st);
            p.line_segment(
                [c + Vec2::new(0.0, s * 0.02), c + Vec2::new(0.0, s * 0.22)],
                st,
            );
            p.circle_filled(c + Vec2::new(0.0, -s * 0.16), w * 0.9, color);
        }
        Icon::Calib => {
            // 抬升距离：地面 + 向上箭头 + 光点
            let base = c + Vec2::new(0.0, s * 0.34);
            p.line_segment(
                [base + Vec2::new(-s * 0.40, 0.0), base + Vec2::new(s * 0.40, 0.0)],
                Stroke::new(w, color),
            );
            p.line_segment(
                [c + Vec2::new(0.0, s * 0.30), c + Vec2::new(0.0, -s * 0.26)],
                st,
            );
            p.add(Shape::convex_polygon(
                vec![
                    c + Vec2::new(0.0, -s * 0.42),
                    c + Vec2::new(-s * 0.16, -s * 0.20),
                    c + Vec2::new(s * 0.16, -s * 0.20),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Chevron => {
            let d = s * 0.16;
            p.line_segment(
                [
                    c + Vec2::new(-d * 0.6, -d),
                    c + Vec2::new(d * 0.6, 0.0),
                ],
                st,
            );
            p.line_segment(
                [c + Vec2::new(d * 0.6, 0.0), c + Vec2::new(-d * 0.6, d)],
                st,
            );
        }
    }
}

// ---------------- 滑杆 ----------------
#[allow(clippy::too_many_arguments)]
pub fn slider(
    ui: &mut Ui,
    id: Id,
    value: &mut f32,
    min: f32,
    max: f32,
    step: f32,
    width: f32,
    accent: Color32,
) -> Response {
    let h = 26.0;
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(width, h), Sense::click_and_drag());
    resp = resp.on_hover_cursor(CursorIcon::PointingHand);

    let pad = 9.0;
    let track = Rect::from_min_max(
        Pos2::new(rect.left() + pad, rect.center().y - 2.5),
        Pos2::new(rect.right() - pad, rect.center().y + 2.5),
    );

    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
            let mut v = min + t * (max - min);
            if step > 0.0 {
                v = (v / step).round() * step;
            }
            v = v.clamp(min, max);
            if (v - *value).abs() > f32::EPSILON {
                *value = v;
                resp.mark_changed();
            }
        }
    }

    let t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
    let x = track.left() + track.width() * t;
    let p = ui.painter();
    let active = resp.hovered() || resp.dragged();

    p.rect_filled(track, Rounding::same(3.0), TRACK);
    if x > track.left() + 0.5 {
        let fill = Rect::from_min_max(track.min, Pos2::new(x, track.max.y));
        p.rect_filled(fill, Rounding::same(3.0), accent);
        if active {
            p.rect_filled(
                Rect::from_min_max(
                    Pos2::new(fill.left(), fill.top() - 3.0),
                    Pos2::new(fill.right(), fill.bottom() + 3.0),
                ),
                Rounding::same(4.0),
                alpha(accent, 40),
            );
        }
    }
    if active {
        p.circle_filled(Pos2::new(x, rect.center().y), 11.0, alpha(accent, 26));
    }
    p.circle_filled(Pos2::new(x, rect.center().y), 7.0, BG);
    p.circle_stroke(Pos2::new(x, rect.center().y), 6.0, Stroke::new(2.0, accent));
    let _ = id;
    resp
}

// ---------------- 开关 ----------------
pub fn toggle(ui: &mut Ui, id: Id, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(42.0, 23.0), Sense::click());
    resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(id, *on, 0.13);
    let p = ui.painter();
    let track = Rect::from_center_size(rect.center(), Vec2::new(42.0, 23.0));
    let bg = mix(TRACK, ACCENT, t);
    p.rect_filled(track, Rounding::same(11.5), bg);
    if *on {
        p.rect_filled(
            track.expand(1.5),
            Rounding::same(13.0),
            alpha(ACCENT, 18),
        );
    }
    let knob_x = track.left() + 11.5 + (track.width() - 23.0) * t;
    p.circle_filled(Pos2::new(knob_x, track.center().y), 8.5, if *on { BG } else { mix(DIM, TRACK, 0.4) });
    resp
}

// ---------------- 分段选择 ----------------
pub fn segmented(ui: &mut Ui, id: Id, labels: &[&str], selected: &mut usize, width: f32) -> Response {
    let h = 32.0;
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(width, h), Sense::hover());
    if labels.is_empty() {
        return resp;
    }
    let p = ui.painter();
    p.rect_filled(rect, Rounding::same(9.0), Color32::from_rgb(0x0C, 0x10, 0x0E));
    p.rect_stroke(rect, Rounding::same(9.0), Stroke::new(1.0, BORDER));
    let seg_w = rect.width() / labels.len() as f32;
    for (i, l) in labels.iter().enumerate() {
        let seg = Rect::from_min_size(
            Pos2::new(rect.left() + seg_w * i as f32, rect.top()),
            Vec2::new(seg_w, rect.height()),
        );
        let inner = seg.shrink(3.0);
        let item = ui.interact(seg, id.with(i), Sense::click());
        if item.clicked() && *selected != i {
            *selected = i;
            resp.mark_changed();
        }
        if *selected == i {
            p.rect_filled(inner, Rounding::same(7.0), CARD_HI);
            p.rect_stroke(inner, Rounding::same(7.0), Stroke::new(1.0, alpha(ACCENT, 90)));
        } else if item.hovered() {
            p.rect_filled(inner, Rounding::same(7.0), alpha(Color32::WHITE, 6));
        }
        let color = if *selected == i { ACCENT } else { DIM };
        p.text(
            seg.center(),
            Align2::CENTER_CENTER,
            *l,
            FontId::new(12.5, egui::FontFamily::Proportional),
            color,
        );
    }
    resp
}

// ---------------- 环形电量 ----------------
pub fn gauge(ui: &mut Ui, size: f32, value_text: &str, sub: &str, frac: f32, color: Color32, charging: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let c = rect.center();
    let r = size / 2.0 - 9.0;
    let p = ui.painter();
    let start = -std::f32::consts::FRAC_PI_2;
    let end = start + std::f32::consts::TAU * frac.clamp(0.0, 1.0);
    let track_w = 9.0;

    p.circle_stroke(c, r, Stroke::new(track_w, TRACK));
    // 光晕
    for (k, a) in [(6.0, 22u8), (3.5, 34), (1.5, 60)] {
        if frac > 0.005 {
            let pts = arc_points(c, r, start, end, 48);
            p.add(Shape::line(pts, Stroke::new(track_w + k, alpha(color, a))));
        }
    }
    if frac > 0.005 {
        let pts = arc_points(c, r, start, end, 48);
        p.add(Shape::line(pts, Stroke::new(track_w, color)));
        let head = c + Vec2::new(end.cos(), end.sin()) * r;
        p.circle_filled(head, track_w / 2.0, color);
        let tail = c + Vec2::new(start.cos(), start.sin()) * r;
        p.circle_filled(tail, track_w / 2.0, color);
    }

    p.text(
        c + Vec2::new(0.0, -3.0),
        Align2::CENTER_CENTER,
        value_text,
        FontId::new(size * 0.24, egui::FontFamily::Name("semi".into())),
        TEXT,
    );
    p.text(
        c + Vec2::new(0.0, size * 0.16),
        Align2::CENTER_CENTER,
        sub.to_string(),
        FontId::new(10.5, egui::FontFamily::Proportional),
        MUTED,
    );
    if charging {
        let bolt = Rect::from_center_size(
            Pos2::new(c.x, c.y - size * 0.03),
            Vec2::splat(size * 0.16),
        );
        // 充电时用闪电替换数字左侧的视觉重心
        icon(p, bolt.translate(Vec2::new(-size * 0.30, 0.0)), Icon::Bolt, alpha(ACCENT, 220));
    }
    resp
}

// ---------------- 按钮 ----------------
fn button_impl(ui: &mut Ui, text: &str, ty: &Type, filled: bool, width: Option<f32>, danger: bool) -> Response {
    let font = ty.body.clone();
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font.clone(), TEXT);
    let w = width.unwrap_or(galley.size().x + 30.0);
    let h = 32.0;
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    let p = ui.painter();
    let base = if danger { DANGER } else { ACCENT };
    if filled {
        let bg = if resp.hovered() { mix(base, Color32::WHITE, 0.12) } else { base };
        p.rect_filled(rect, Rounding::same(8.0), bg);
        p.text(
            rect.center(),
            Align2::CENTER_CENTER,
            text.to_string(),
            font,
            if danger { Color32::WHITE } else { ACCENT_DARK },
        );
    } else {
        let bg = if resp.hovered() {
            alpha(base, 26)
        } else if resp.is_pointer_button_down_on() {
            alpha(base, 40)
        } else {
            Color32::TRANSPARENT
        };
        p.rect_filled(rect, Rounding::same(8.0), bg);
        p.rect_stroke(
            rect,
            Rounding::same(8.0),
            Stroke::new(1.0, if resp.hovered() { alpha(base, 140) } else { BORDER_HI }),
        );
        p.text(
            rect.center(),
            Align2::CENTER_CENTER,
            text.to_string(),
            font,
            if resp.hovered() { base } else { DIM },
        );
    }
    resp
}

pub fn accent_button(ui: &mut Ui, ty: &Type, text: &str) -> Response {
    button_impl(ui, text, ty, true, None, false)
}

pub fn accent_button_w(ui: &mut Ui, ty: &Type, text: &str, w: f32) -> Response {
    button_impl(ui, text, ty, true, Some(w), false)
}

pub fn ghost_button(ui: &mut Ui, ty: &Type, text: &str) -> Response {
    button_impl(ui, text, ty, false, None, false)
}

pub fn ghost_button_w(ui: &mut Ui, ty: &Type, text: &str, w: f32) -> Response {
    button_impl(ui, text, ty, false, Some(w), false)
}

pub fn danger_button(ui: &mut Ui, ty: &Type, text: &str) -> Response {
    button_impl(ui, text, ty, false, None, true)
}

// ---------------- 窗口按钮 ----------------
pub fn window_button(ui: &mut Ui, ic: Icon, danger: bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(42.0, 30.0), Sense::click());
    resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    if resp.hovered() {
        ui.painter().rect_filled(
            rect,
            Rounding::same(7.0),
            if danger {
                Color32::from_rgb(0xC4, 0x2B, 0x2B)
            } else {
                alpha(Color32::WHITE, 14)
            },
        );
    }
    let col = if resp.hovered() && danger {
        Color32::WHITE
    } else if resp.hovered() {
        TEXT
    } else {
        DIM
    };
    icon(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(15.0)), ic, col);
    resp
}

// ---------------- 导航 ----------------
pub fn nav_item(ui: &mut Ui, ty: &Type, ic: Icon, label: &str, selected: bool, width: f32) -> Response {
    let h = 38.0;
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(width, h), Sense::click());
    resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, Rounding::same(9.0), alpha(ACCENT, 18));
        let bar = Rect::from_min_size(
            Pos2::new(rect.left() + 1.0, rect.center().y - 9.0),
            Vec2::new(3.0, 18.0),
        );
        p.rect_filled(bar, Rounding::same(2.0), ACCENT);
    } else if resp.hovered() {
        p.rect_filled(rect, Rounding::same(9.0), alpha(Color32::WHITE, 5));
    }
    let col = if selected { ACCENT } else { mix(DIM, TEXT, if resp.hovered() { 0.5 } else { 0.0 }) };
    icon(
        p,
        Rect::from_center_size(Pos2::new(rect.left() + 26.0, rect.center().y), Vec2::splat(16.0)),
        ic,
        col,
    );
    p.text(
        Pos2::new(rect.left() + 46.0, rect.center().y),
        Align2::LEFT_CENTER,
        label.to_string(),
        ty.body.clone(),
        col,
    );
    resp
}

// ---------------- 数值显示 ----------------
pub fn stat(ui: &mut Ui, ty: &Type, caption: &str, value: impl Into<String>, unit: &str) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(caption).font(ty.tiny.clone()).color(MUTED));
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(value).font(ty.num.clone()).color(TEXT));
            if !unit.is_empty() {
                ui.add_space(-2.0);
                ui.label(egui::RichText::new(unit).font(ty.small.clone()).color(MUTED));
            }
        });
    });
}
