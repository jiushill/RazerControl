//! 六个页面：概览 / DPI / 性能 / 按键 / 灯效 / 设置

use crate::app::App;
use crate::art;
use crate::backend::Req;
use crate::config::Action;
use crate::photo;
use crate::theme;
use crate::theme::*;
use crate::widgets::{self, Icon};
use egui::{Align, Align2, Color32, FontId, Layout, Pos2, Rect, Rounding, Sense, Stroke, Ui, Vec2};
use rz_core::DeviceState;

// ---------------- 通用 ----------------
fn page_head(app: &App, ui: &mut Ui, title: &str, sub: &str) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(title)
                    .font(app.ty.title.clone())
                    .color(TEXT),
            );
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(sub)
                    .font(app.ty.small.clone())
                    .color(MUTED),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (txt, col) = if app.asleep {
                ("休眠中 · 移动鼠标唤醒", WARN)
            } else if app.connected {
                ("设备已连接", ACCENT)
            } else {
                ("未检测到设备", DANGER)
            };
            theme::pill(ui, &app.ty, txt, col);
        });
    });
    ui.add_space(14.0);
}

fn stage_badge(ui: &mut Ui, ty: &Type, n: usize, active: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
    let p = ui.painter();
    if active {
        p.circle_filled(rect.center(), 13.0, alpha(ACCENT, 40));
        p.circle_stroke(rect.center(), 12.0, Stroke::new(1.0, alpha(ACCENT, 120)));
    } else if resp.hovered() {
        p.circle_filled(rect.center(), 13.0, alpha(Color32::WHITE, 8));
    }
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        (n + 1).to_string(),
        FontId::new(11.5, ty.semi.clone()),
        if active { ACCENT } else { DIM },
    );
    resp.clicked()
}

fn empty_state(ui: &mut Ui, app: &App, icon: Icon, title: &str, body: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 210.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, Rounding::same(14.0), CARD);
    p.rect_stroke(rect, Rounding::same(14.0), Stroke::new(1.0, BORDER));
    widgets::icon(
        p,
        Rect::from_center_size(
            Pos2::new(rect.center().x, rect.top() + 62.0),
            Vec2::splat(34.0),
        ),
        icon,
        alpha(DIM, 150),
    );
    p.text(
        Pos2::new(rect.center().x, rect.top() + 100.0),
        Align2::CENTER_CENTER,
        title,
        app.ty.h1.clone(),
        DIM,
    );
    p.text(
        Pos2::new(rect.center().x, rect.top() + 126.0),
        Align2::CENTER_CENTER,
        body,
        app.ty.small.clone(),
        MUTED,
    );
}

// ---------------- 概览 ----------------
pub fn dashboard(app: &mut App, ui: &mut Ui) {
    page_head(app, ui, "概览", "实时设备状态与快捷调节");
    let st = app.state.clone().unwrap_or_default();
    let unknown = !st.caps.dpi && !st.caps.poll_rate && !st.caps.battery;
    let dash = |v: String| if unknown { "—".to_string() } else { v };

    theme::card(ui, CardStyle::new().pad(20.0), |ui| {
        ui.horizontal_top(|ui| {
            // 设备产品图（无匹配素材时回退到矢量插画）
            let (art_rect, _) = ui.allocate_exact_size(Vec2::new(176.0, 192.0), Sense::hover());
            match (app.photo(), photo::has_photo_for(st.pid)) {
                (Some(p), true) => {
                    photo::draw(ui.painter(), art_rect, p, None, None);
                }
                _ => {
                    art::draw_mouse(
                        ui.painter(),
                        art_rect.shrink2(Vec2::new(16.0, 8.0)),
                        ACCENT,
                        None,
                        None,
                    );
                }
            }

            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(if st.name.is_empty() {
                        "等待雷蛇设备…"
                    } else {
                        &st.name
                    })
                    .font(app.ty.title.clone())
                    .color(TEXT),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let (mode, col) = if unknown {
                        ("状态未知", MUTED)
                    } else if st.wireless {
                        ("2.4G 无线", ACCENT)
                    } else {
                        ("USB 有线", INFO)
                    };
                    theme::pill(ui, &app.ty, mode, col);
                    if !st.firmware.is_empty() {
                        theme::pill(ui, &app.ty, &format!("固件 {}", st.firmware), DIM);
                    }
                    match st.edition {
                        0x80 => {
                            theme::pill(ui, &app.ty, "白色", mix(TEXT, INFO, 0.15));
                        }
                        0x00 => {
                            theme::pill(ui, &app.ty, "黑色", mix(DIM, BG, 0.3));
                        }
                        _ => {}
                    }
                    if st.has_rgb {
                        theme::pill(ui, &app.ty, "CHROMA", mix(ACCENT, INFO, 0.5));
                    }
                });
                ui.add_space(18.0);
                ui.horizontal(|ui| {
                    widgets::stat(ui, &app.ty, "当前 DPI", dash(format!("{}", st.dpi.0)), "");
                    ui.add_space(22.0);
                    ui.vertical(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(1.0, 34.0), Sense::hover());
                        ui.painter().rect_filled(rect, 0.0, BORDER);
                    });
                    ui.add_space(22.0);
                    widgets::stat(ui, &app.ty, "轮询率", dash(format!("{}", st.poll_hz)), "Hz");
                    ui.add_space(22.0);
                    ui.vertical(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(1.0, 34.0), Sense::hover());
                        ui.painter().rect_filled(rect, 0.0, BORDER);
                    });
                    ui.add_space(22.0);
                    let stage_val = if st.profile.stages.is_empty() {
                        "—".to_string()
                    } else {
                        format!("{}", st.profile.active.max(1))
                    };
                    let stage_unit = if st.profile.stages.is_empty() {
                        String::new()
                    } else {
                        format!("/ {}", st.profile.stages.len())
                    };
                    widgets::stat(ui, &app.ty, "活动段", stage_val, &stage_unit);
                });
            });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // 休眠时电量不可读：显示 — 而不是过期数值
                let known = st.caps.battery && !app.asleep;
                let pct = st.battery.percent;
                let col = if !known {
                    MUTED
                } else if pct < 15 {
                    WARN
                } else {
                    ACCENT
                };
                let sub = if !known {
                    "无数据"
                } else if st.battery.charging {
                    "充电中"
                } else {
                    "电池"
                };
                let pct_txt = if known {
                    format!("{}", pct)
                } else {
                    "—".to_string()
                };
                widgets::gauge(
                    ui,
                    118.0,
                    &pct_txt,
                    sub,
                    if known { pct as f32 / 100.0 } else { 0.0 },
                    col,
                    known && st.battery.charging,
                );
            });
        });
    });

    ui.add_space(12.0);

    ui.columns(2, |cols| {
        theme::card(&mut cols[0], CardStyle::new(), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("DPI 阶梯")
                        .font(app.ty.h2.clone())
                        .color(TEXT),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("点击切换活动段")
                            .font(app.ty.tiny.clone())
                            .color(MUTED),
                    );
                });
            });
            ui.add_space(6.0);
            let stages = st.profile.stages.clone();
            let active = st.profile.active;
            let mut clicked_stage: Option<u8> = None;
            for (i, s) in stages.iter().enumerate() {
                let is_active = active as usize == i + 1;
                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
                let p = ui.painter();
                if is_active {
                    p.rect_filled(rect.expand2(Vec2::new(2.0, 0.0)), Rounding::same(7.0), alpha(ACCENT, 16));
                } else if resp.hovered() {
                    p.rect_filled(rect.expand2(Vec2::new(2.0, 0.0)), Rounding::same(7.0), alpha(Color32::WHITE, 5));
                }
                p.circle_stroke(
                    Pos2::new(rect.left() + 14.0, rect.center().y),
                    9.0,
                    Stroke::new(1.2, if is_active { ACCENT } else { BORDER_HI }),
                );
                p.text(
                    Pos2::new(rect.left() + 14.0, rect.center().y),
                    Align2::CENTER_CENTER,
                    (i + 1).to_string(),
                    FontId::new(10.0, app.ty.semi.clone()),
                    if is_active { ACCENT } else { DIM },
                );
                p.text(
                    Pos2::new(rect.left() + 34.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    format!("{} DPI", s.x),
                    FontId::new(12.5, egui::FontFamily::Proportional),
                    if is_active { TEXT } else { DIM },
                );
                if is_active {
                    p.text(
                        Pos2::new(rect.right() - 6.0, rect.center().y),
                        Align2::RIGHT_CENTER,
                        "使用中",
                        FontId::new(10.0, app.ty.semi.clone()),
                        ACCENT,
                    );
                }
                if resp.clicked() {
                    clicked_stage = Some((i + 1) as u8);
                }
            }
            if let Some(n) = clicked_stage {
                app.backend.req(Req::SetActiveStage(n));
            }
        });

        theme::card(&mut cols[1], CardStyle::new(), |ui| {
            ui.label(
                egui::RichText::new("性能")
                    .font(app.ty.h2.clone())
                    .color(TEXT),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("轮询率")
                    .font(app.ty.small.clone())
                    .color(MUTED),
            );
            ui.add_space(4.0);
            let rates: Vec<u16> = [125u16, 250, 500, 1000, 2000, 4000, 8000]
                .into_iter()
                .filter(|h| *h <= st.max_poll.max(1000))
                .collect();
            let labels: Vec<String> = rates.iter().map(|h| format!("{}", h)).collect();
            let labels_ref: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
            let mut sel = rates.iter().position(|h| *h == st.poll_hz).unwrap_or(3);
            let w = ui.available_width();
            if widgets::segmented(ui, egui::Id::new("poll_seg"), &labels_ref, &mut sel, w).changed() {
                app.backend.req(Req::SetPoll(rates[sel]));
            }
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new("单位 Hz · 更高回报率带来更低延迟")
                    .font(app.ty.tiny.clone())
                    .color(MUTED),
            );

            ui.add_space(14.0);
            divider(ui);
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("休眠时间")
                        .font(app.ty.small.clone())
                        .color(MUTED),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{} 分钟", (st.sleep_secs / 60).max(1)))
                            .font(app.ty.small.clone())
                            .color(TEXT),
                    );
                });
            });
            let mut sleep_min = (st.sleep_secs as f32 / 60.0).clamp(1.0, 15.0);
            let w = ui.available_width();
            if widgets::slider(ui, egui::Id::new("sleep"), &mut sleep_min, 1.0, 15.0, 1.0, w, ACCENT)
                .drag_stopped()
            {
                app.backend.req(Req::SetSleepMin(sleep_min as u16));
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("低电提示阈值")
                        .font(app.ty.small.clone())
                        .color(MUTED),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{}%", st.low_battery_percent))
                            .font(app.ty.small.clone())
                            .color(TEXT),
                    );
                });
            });
            let mut low = st.low_battery_percent.max(5) as f32;
            let w = ui.available_width();
            if widgets::slider(ui, egui::Id::new("lowbatt"), &mut low, 5.0, 50.0, 1.0, w, WARN)
                .drag_stopped()
            {
                app.backend.req(Req::SetLowBatt(low as u8));
            }
        });
    });
}

// ---------------- DPI ----------------
pub fn dpi(app: &mut App, ui: &mut Ui) {
    page_head(app, ui, "DPI", "可调灵敏度阶梯，直接写入设备存储");
    let st = app.state.clone().unwrap_or_default();
    let max_dpi = if st.max_dpi == 0 { 30000.0 } else { st.max_dpi as f32 };

    if app.dpi_draft.len() != st.profile.stages.len() && !st.profile.stages.is_empty() {
        app.dpi_draft = st.profile.stages.iter().map(|s| s.x as f32).collect();
    }

    theme::card(ui, CardStyle::new().pad(20.0), |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("灵敏度阶梯")
                    .font(app.ty.h2.clone())
                    .color(TEXT),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut sync = app.cfg.sync_xy;
                if widgets::toggle(ui, egui::Id::new("sync_xy"), &mut sync).changed() {
                    app.cfg.sync_xy = sync;
                    let _ = app.cfg.save();
                }
                ui.label(
                    egui::RichText::new("同步 X/Y")
                        .font(app.ty.small.clone())
                        .color(DIM),
                );
            });
        });
        ui.add_space(10.0);

        let active = st.profile.active;
        let count = st.profile.stages.len().max(1);
        let mut commit: Option<(usize, u16)> = None;
        let mut activate: Option<u8> = None;
        let row_w = ui.available_width();

        for i in 0..count {
            let mut v = app.dpi_draft.get(i).copied().unwrap_or(800.0);
            ui.horizontal(|ui| {
                if stage_badge(ui, &app.ty, i, active as usize == i + 1) {
                    activate = Some((i + 1) as u8);
                }
                let slider_w = (row_w - 26.0 - 88.0 - 84.0 - 16.0).max(80.0);
                let r = widgets::slider(
                    ui,
                    egui::Id::new(("dpi_slider", i)),
                    &mut v,
                    if i == 0 { 100.0 } else { 100.0 },
                    max_dpi,
                    50.0,
                    slider_w,
                    if active as usize == i + 1 { ACCENT } else { mix(ACCENT, DIM, 0.55) },
                );
                if r.changed() {
                    app.dpi_draft[i] = v;
                }
                if r.drag_stopped() || r.lost_focus() {
                    commit = Some((i, v as u16));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::ghost_button(ui, &app.ty, "激活").clicked() {
                        activate = Some((i + 1) as u8);
                    }
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!("{}", v as u16))
                            .font(app.ty.mono.clone())
                            .color(if active as usize == i + 1 { TEXT } else { DIM }),
                    );
                });
            });
            if i + 1 < count {
                ui.add_space(2.0);
            }
        }

        if let Some((i, val)) = commit {
            app.backend.req(Req::SetStageDpi(i, val));
        }
        if let Some(n) = activate {
            app.backend.req(Req::SetActiveStage(n));
        }

        ui.add_space(10.0);
        divider(ui);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("线性修正")
                    .font(app.ty.small.clone())
                    .color(MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::ghost_button(ui, &app.ty, "写入全部").clicked() {
                    let vals: Vec<u16> = app.dpi_draft.iter().map(|v| *v as u16).collect();
                    app.backend.req(Req::SetStages(vals));
                }
                if widgets::ghost_button(ui, &app.ty, "恢复默认").clicked() {
                    let defaults = [400u16, 800, 1600, 3200, 6400];
                    let n = st.profile.stages.len().max(1);
                    app.backend.req(Req::SetStages(
                        defaults.iter().take(n).copied().collect(),
                    ));
                }
            });
        });
    });

    ui.add_space(12.0);

    ui.columns(2, |cols| {
        theme::card(&mut cols[0], CardStyle::new(), |ui| {
            ui.label(
                egui::RichText::new("实时灵敏度")
                    .font(app.ty.h2.clone())
                    .color(TEXT),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{}", st.dpi.0))
                        .font(app.ty.big.clone())
                        .color(ACCENT),
                );
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.add_space(14.0);
                    ui.label(
                        egui::RichText::new("DPI")
                            .font(app.ty.small.clone())
                            .color(MUTED),
                    );
                    ui.label(
                        egui::RichText::new(format!("{:.1} cm/360° @1.0", 41568.0 / st.dpi.0.max(1) as f32))
                            .font(app.ty.tiny.clone())
                            .color(MUTED),
                    );
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if widgets::ghost_button(ui, &app.ty, "上一段").clicked() {
                    let n = if st.profile.active <= 1 {
                        st.profile.stages.len() as u8
                    } else {
                        st.profile.active - 1
                    };
                    app.backend.req(Req::SetActiveStage(n));
                }
                if widgets::ghost_button(ui, &app.ty, "下一段").clicked() {
                    let n = if st.profile.active as usize >= st.profile.stages.len() {
                        1
                    } else {
                        st.profile.active + 1
                    };
                    app.backend.req(Req::SetActiveStage(n));
                }
            });
        });

        theme::card(&mut cols[1], CardStyle::new(), |ui| {
            ui.label(
                egui::RichText::new("阶梯曲线")
                    .font(app.ty.h2.clone())
                    .color(TEXT),
            );
            ui.add_space(6.0);
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 104.0), Sense::hover());
            let p = ui.painter().with_clip_rect(rect);
            let n = st.profile.stages.len().max(1);
            let maxv = st.profile.stages.iter().map(|s| s.x).max().unwrap_or(1).max(1) as f32;
            let base = rect.top() + 100.0;
            for i in 0..n {
                let w = rect.width() / n as f32;
                let x0 = rect.left() + w * i as f32 + 7.0;
                let x1 = rect.left() + w * (i as f32 + 1.0) - 7.0;
                let v = st.profile.stages.get(i).map(|s| s.x).unwrap_or(0) as f32;
                let h = (v / maxv) * 96.0;
                let is_active = st.profile.active as usize == i + 1;
                let track = Rect::from_min_max(Pos2::new(x0, base - 96.0), Pos2::new(x1, base));
                p.rect_filled(track, Rounding::same(6.0), TRACK);
                let fill = Rect::from_min_max(Pos2::new(x0, base - h.max(4.0)), Pos2::new(x1, base));
                p.rect_filled(
                    fill,
                    Rounding::same(6.0),
                    if is_active { ACCENT } else { alpha(ACCENT, 70) },
                );
                p.text(
                    Pos2::new((x0 + x1) * 0.5, base + 2.0),
                    Align2::CENTER_TOP,
                    (i + 1).to_string(),
                    FontId::new(10.0, app.ty.semi.clone()),
                    if is_active { ACCENT } else { MUTED },
                );
            }
        });
    });
}

// ---------------- 性能 ----------------
pub fn performance(app: &mut App, ui: &mut Ui) {
    page_head(app, ui, "性能", "回报率、省电与连接状态");
    let st = app.state.clone().unwrap_or_default();

    theme::card(ui, CardStyle::new().pad(20.0), |ui| {
        ui.label(
            egui::RichText::new("轮询率")
                .font(app.ty.h2.clone())
                .color(TEXT),
        );
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("设备每秒上报给系统的次数，越高越跟手，也越费电")
                .font(app.ty.small.clone())
                .color(MUTED),
        );
        ui.add_space(10.0);
        let rates: Vec<u16> = [125u16, 250, 500, 1000, 2000, 4000, 8000]
            .into_iter()
            .filter(|h| *h <= st.max_poll.max(1000))
            .collect();
        let labels: Vec<String> = rates.iter().map(|h| format!("{} Hz", h)).collect();
        let labels_ref: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
        let mut sel = rates.iter().position(|h| *h == st.poll_hz).unwrap_or(3);
        let w = ui.available_width();
        if widgets::segmented(ui, egui::Id::new("poll_seg2"), &labels_ref, &mut sel, w).changed() {
            app.backend.req(Req::SetPoll(rates[sel]));
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(format!(
                "当前编码 0x{:02X} · 设备上限 {} Hz",
                st.poll_code, st.max_poll
            ))
            .font(app.ty.tiny.clone())
            .color(MUTED),
        );
    });

    ui.add_space(12.0);
    ui.columns(2, |cols| {
        theme::card(&mut cols[0], CardStyle::new(), |ui| {
            ui.label(
                egui::RichText::new("省电")
                    .font(app.ty.h2.clone())
                    .color(TEXT),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("休眠时间")
                        .font(app.ty.small.clone())
                        .color(MUTED),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{} 分钟", (st.sleep_secs / 60).max(1)))
                            .font(app.ty.mono.clone())
                            .color(TEXT),
                    );
                });
            });
            let mut sleep_min = (st.sleep_secs as f32 / 60.0).clamp(1.0, 15.0);
            let w = ui.available_width();
            if widgets::slider(ui, egui::Id::new("sleep2"), &mut sleep_min, 1.0, 15.0, 1.0, w, ACCENT)
                .drag_stopped()
            {
                app.backend.req(Req::SetSleepMin(sleep_min as u16));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("低电提示")
                        .font(app.ty.small.clone())
                        .color(MUTED),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{}%", st.low_battery_percent))
                            .font(app.ty.mono.clone())
                            .color(TEXT),
                    );
                });
            });
            let mut low = st.low_battery_percent.max(5) as f32;
            let w = ui.available_width();
            if widgets::slider(ui, egui::Id::new("lowbatt2"), &mut low, 5.0, 50.0, 1.0, w, WARN)
                .drag_stopped()
            {
                app.backend.req(Req::SetLowBatt(low as u8));
            }
        });

        theme::card(&mut cols[1], CardStyle::new(), |ui| {
            ui.label(
                egui::RichText::new("电源与连接")
                    .font(app.ty.h2.clone())
                    .color(TEXT),
            );
            ui.add_space(10.0);
            ui.horizontal_top(|ui| {
                widgets::gauge(
                    ui,
                    104.0,
                    &format!("{}", if st.caps.battery { st.battery.percent } else { 100 }),
                    if st.battery.charging { "充电中" } else { "电池" },
                    if st.caps.battery {
                        st.battery.percent as f32 / 100.0
                    } else {
                        1.0
                    },
                    if st.battery.percent < 15 && st.caps.battery {
                        WARN
                    } else {
                        ACCENT
                    },
                    st.battery.charging,
                );
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.add_space(6.0);
                    theme::key_value(ui, &app.ty, "连接方式", if st.wireless { "2.4G 无线" } else { "USB 有线" });
                    theme::key_value(ui, &app.ty, "充电状态", if st.battery.charging { "充电中" } else { "放电" });
                    theme::key_value(ui, &app.ty, "固件版本", if st.firmware.is_empty() { "—" } else { &st.firmware });
                    theme::key_value(ui, &app.ty, "序列号", if st.serial.is_empty() { "—" } else { &st.serial });
                });
            });
        });
    });
}

// ---------------- 校准 ----------------
pub fn calibration(app: &mut App, ui: &mut Ui) {
    page_head(app, ui, "校准", "抬升距离、着陆距离与非对称中止");
    let st = app.state.clone().unwrap_or_default();

    if !st.caps.calibration {
        empty_state(
            ui,
            app,
            Icon::Calib,
            "该型号未提供校准接口",
            "当前型号未验证 0x0B 校准写入；目前只有 DeathAdder V3 系列（00B6/00B7/00B8/00B9）实测可用。",
        );
        ui.add_space(12.0);
        theme::card(ui, CardStyle::new(), |ui| {
            ui.label(
                egui::RichText::new("手动重置")
                    .font(app.ty.h2.clone())
                    .color(TEXT),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("同时按住鼠标左键、右键和滚轮约 7 秒，即可把传感器校准恢复为出厂值。")
                    .font(app.ty.small.clone())
                    .color(MUTED),
            );
        });
        return;
    }

    // ---- 追踪距离（三档）----
    theme::card(ui, CardStyle::new().pad(20.0), |ui| {
        ui.label(
            egui::RichText::new("追踪距离")
                .font(app.ty.h2.clone())
                .color(TEXT),
        );
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("抬起鼠标后光标停止响应的距离；越高越早停")
                .font(app.ty.small.clone())
                .color(MUTED),
        );
        ui.add_space(10.0);
        let levels = ["低", "中等", "高"];
        let mut sel = app.cfg.calib_tracking.min(2) as usize;
        let w = ui.available_width();
        if widgets::segmented(ui, egui::Id::new("calib_tracking"), &levels, &mut sel, w).changed() {
            app.cfg.calib_tracking = sel as u8;
            let _ = app.cfg.save();
            app.backend.req(Req::SetTracking(sel as u8));
        }
    });

    ui.add_space(12.0);

    // ---- 非对称中止 + 抬升/着陆 ----
    theme::card(ui, CardStyle::new().pad(20.0), |ui| {
        ui.horizontal(|ui| {
            let mut on = app.cfg.calib_asym;
            if widgets::toggle(ui, egui::Id::new("calib_asym"), &mut on).changed() {
                app.cfg.calib_asym = on;
                let _ = app.cfg.save();
                app.backend.req(Req::SetAsymmetric(on));
            }
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("启用非对称中止")
                        .font(app.ty.h2.clone())
                        .color(TEXT),
                );
                ui.label(
                    egui::RichText::new("分别设置抬升与着陆的响应距离，避免抬鼠时过早/过晚停止")
                        .font(app.ty.tiny.clone())
                        .color(MUTED),
                );
            });
        });

        ui.add_space(12.0);
        let enabled = app.cfg.calib_asym;
        let dim = if enabled { TEXT } else { MUTED };

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("抬升距离")
                    .font(app.ty.small.clone())
                    .color(if enabled { MUTED } else { MUTED }),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{}", app.cfg.calib_lift))
                        .font(app.ty.mono.clone())
                        .color(dim),
                );
            });
        });
        let mut lift = app.cfg.calib_lift.clamp(1.0 as u8, 26) as f32;
        let w = ui.available_width();
        if widgets::slider(ui, egui::Id::new("calib_lift"), &mut lift, 1.0, 26.0, 1.0, w,
                           if enabled { ACCENT } else { mix(ACCENT, CARD, 0.65) })
            .drag_stopped() && enabled
        {
            let l = lift as u8;
            if l <= app.cfg.calib_land {
                app.cfg.calib_land = l.saturating_sub(1).max(1);
            }
            app.cfg.calib_lift = l;
            let _ = app.cfg.save();
            app.backend.req(Req::SetLiftLand(app.cfg.calib_lift, app.cfg.calib_land));
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("着陆距离")
                    .font(app.ty.small.clone())
                    .color(MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{}", app.cfg.calib_land))
                        .font(app.ty.mono.clone())
                        .color(dim),
                );
            });
        });
        let max_land = (app.cfg.calib_lift.saturating_sub(1)).max(1) as f32;
        let mut land = (app.cfg.calib_land.clamp(1, max_land as u8)) as f32;
        let w = ui.available_width();
        if widgets::slider(ui, egui::Id::new("calib_land"), &mut land, 1.0, max_land, 1.0, w,
                           if enabled { ACCENT } else { mix(ACCENT, CARD, 0.65) })
            .drag_stopped() && enabled
        {
            app.cfg.calib_land = land as u8;
            let _ = app.cfg.save();
            app.backend.req(Req::SetLiftLand(app.cfg.calib_lift, app.cfg.calib_land));
        }
        if !enabled {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("开启「非对称中止」后才能调整这两项")
                    .font(app.ty.tiny.clone())
                    .color(MUTED),
            );
        }
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("着陆距离需要小于抬升距离，否则部分鼠标垫会不兼容")
                .font(app.ty.tiny.clone())
                .color(MUTED),
        );
    });

    ui.add_space(12.0);
    ui.columns(2, |cols| {
        theme::card(&mut cols[0], CardStyle::new(), |ui| {
            ui.label(
                egui::RichText::new("重置")
                    .font(app.ty.h2.clone())
                    .color(TEXT),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("通过同时按住左键、右键和滚轮 7 秒钟，可以手动重置你的鼠标。")
                    .font(app.ty.small.clone())
                    .color(MUTED),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if widgets::ghost_button(ui, &app.ty, "写入推荐值").clicked() {
                    app.cfg.calib_tracking = 1;
                    app.cfg.calib_asym = false;
                    app.cfg.calib_lift = 15;
                    app.cfg.calib_land = 8;
                    let _ = app.cfg.save();
                    app.backend.req(Req::SetTracking(1));
                    app.backend.req(Req::SetAsymmetric(false));
                }
            });
        });
        theme::card(&mut cols[1], CardStyle::new(), |ui| {
            ui.label(
                egui::RichText::new("说明")
                    .font(app.ty.h2.clone())
                    .color(TEXT),
            );
            ui.add_space(8.0);
            for line in [
                "写入命令：0x0B/0x0B（参数）与 0x0B/0x05（抬升+着陆）",
                "设备只接受写入、不回读，界面显示的是本程序保存的值",
                "抬升/着陆仅在「非对称中止」开启时才会被设备接受",
            ] {
                ui.label(
                    egui::RichText::new(format!("· {}", line))
                        .font(app.ty.small.clone())
                        .color(MUTED),
                );
                ui.add_space(2.0);
            }
        });
    });
}

// ---------------- 按键 ----------------
pub fn buttons(app: &mut App, ui: &mut Ui) {
    page_head(app, ui, "按键", "把侧键映射成任意动作（软件层拦截）");

    theme::card(ui, CardStyle::new().pad(16.0), |ui| {
        ui.horizontal(|ui| {
            let mut enabled = app.cfg.remap_enabled;
            if widgets::toggle(ui, egui::Id::new("remap_on"), &mut enabled).changed() {
                app.cfg.remap_enabled = enabled;
                let _ = app.cfg.save();
                app.sync_hook();
            }
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("启用软件按键映射")
                        .font(app.ty.h2.clone())
                        .color(TEXT),
                );
                ui.label(
                    egui::RichText::new(if crate::hook::is_installed() {
                        "钩子已加载 · 映射仅在本程序运行时生效"
                    } else {
                        "钩子未加载"
                    })
                    .font(app.ty.tiny.clone())
                    .color(MUTED),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::ghost_button(ui, &app.ty, "全部恢复默认").clicked() {
                    app.cfg.remaps = [Action::Pass; 5];
                    let _ = app.cfg.save();
                    app.sync_hook();
                }
            });
        });
    });

    ui.add_space(12.0);

    ui.horizontal_top(|ui| {
        let art_w = 300.0;
        ui.vertical(|ui| {
            ui.set_width(art_w);
            theme::card(ui, CardStyle::new().pad(14.0), |ui| {
                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 330.0), Sense::hover());
                let pid = app.state.as_ref().map(|s| s.pid).unwrap_or(0);
                let hits = match (app.photo(), photo::has_photo_for(pid)) {
                    (Some(p), true) => photo::draw(
                        ui.painter(),
                        rect.shrink2(Vec2::new(24.0, 8.0)),
                        p,
                        Some(app.sel_btn),
                        app.hover_btn,
                    ),
                    _ => art::draw_mouse(
                        ui.painter(),
                        rect.shrink2(Vec2::new(40.0, 6.0)),
                        ACCENT,
                        Some(app.sel_btn),
                        app.hover_btn,
                    ),
                };
                let pointer = ui.input(|i| i.pointer.hover_pos());
                app.hover_btn = pointer.and_then(|p| hits.iter().position(|r| r.contains(p)));
                if resp.hovered() {
                    if let Some(i) = app.hover_btn {
                        if let Some(p) = pointer {
                            if ui.input(|i| i.pointer.any_click()) {
                                app.sel_btn = i;
                            }
                            let _ = p;
                        }
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                }
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(format!(
                        "当前选中：{}",
                        btn_name(app.sel_btn)
                    ))
                    .font(app.ty.small.clone())
                    .color(DIM),
                );
            });
        });

        ui.add_space(10.0);

        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            theme::card(ui, CardStyle::new().pad(18.0), |ui| {
                ui.label(
                    egui::RichText::new("映射表")
                        .font(app.ty.h2.clone())
                        .color(TEXT),
                );
                ui.add_space(8.0);
                let mut changed = false;
                for i in 0..5 {
                    ui.horizontal(|ui| {
                        let active = app.sel_btn == i;
                        let (dot, dresp) =
                            ui.allocate_exact_size(Vec2::splat(12.0), Sense::click());
                        ui.painter().circle_filled(
                            dot.center(),
                            4.0,
                            if active { ACCENT } else { BORDER_HI },
                        );
                        if active {
                            ui.painter()
                                .circle_filled(dot.center(), 7.0, alpha(ACCENT, 40));
                        }
                        let lab = ui.add(
                            egui::Label::new(
                                egui::RichText::new(btn_name(i))
                                    .font(if active { app.ty.h2.clone() } else { app.ty.body.clone() })
                                    .color(if active { TEXT } else { DIM }),
                            )
                            .sense(Sense::click()),
                        );
                        if lab.clicked() || dresp.clicked() {
                            app.sel_btn = i;
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let cur = app.cfg.remaps[i];
                            let mut sel = cur;
                            egui::ComboBox::from_id_salt(("act", i))
                                .selected_text(cur.label())
                                .width(168.0)
                                .show_ui(ui, |ui| {
                                    for a in Action::ALL {
                                        if ui.selectable_label(a == cur, a.label()).clicked() {
                                            sel = a;
                                        }
                                    }
                                });
                            if sel != cur {
                                app.cfg.remaps[i] = sel;
                                changed = true;
                            }
                        });
                    });
                    if i < 4 {
                        ui.add_space(2.0);
                        divider(ui);
                        ui.add_space(2.0);
                    }
                }
                if changed {
                    let _ = app.cfg.save();
                    app.sync_hook();
                }
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("提示：DPI 切换动作会调用驱动指令，其余动作通过合成输入完成。")
                        .font(app.ty.tiny.clone())
                        .color(MUTED),
                );
            });
        });
    });
}

/// 行标题：与 `photo::BUTTON_NAMES` 保持同一顺序
/// （索引 3 = 几何靠前的侧键 = XBUTTON2 = 前进，索引 4 = 靠后 = XBUTTON1 = 后退）
fn btn_name(i: usize) -> &'static str {
    match i {
        0 => "左键",
        1 => "右键",
        2 => "滚轮中键",
        3 => "侧键 · 前进",
        _ => "侧键 · 后退",
    }
}

// ---------------- 灯效 ----------------
pub fn lighting(app: &mut App, ui: &mut Ui) {
    page_head(app, ui, "灯效", "Chroma 灯区控制");
    let st = app.state.clone().unwrap_or_default();
    if !st.has_rgb || !st.caps.lighting {
        empty_state(
            ui,
            app,
            Icon::Light,
            "该设备没有可用的背光模块",
            "当前型号未提供可用的 Chroma 灯效接口；支持灯效的型号会自动启用本页。",
        );
        ui.add_space(12.0);
        theme::card(ui, CardStyle::new(), |ui| {
            ui.label(
                egui::RichText::new("已实现的灯效指令")
                    .font(app.ty.h2.clone())
                    .color(TEXT),
            );
            ui.add_space(6.0);
            for line in [
                "经典 Chroma：亮度 0x0F/0x83 读、0x0F/0x03 写，静态色 0x0F/0x02",
                "DeathAdder V2 Pro：亮度 0x0F/0x84 读、0x0F/0x04 写（LOGO LED 0x04）",
                "V2 Pro 静态色：0x0F/0x02，args = [01,04,01,00,00,01,R,G,B]",
                "自动探测：对应亮度 getter 成功时才判定设备有灯区",
            ] {
                ui.label(
                    egui::RichText::new(format!("· {}", line))
                        .font(app.ty.small.clone())
                        .color(MUTED),
                );
            }
        });
        return;
    }

    theme::card(ui, CardStyle::new().pad(20.0), |ui| {
        ui.label(
            egui::RichText::new("亮度")
                .font(app.ty.h2.clone())
                .color(TEXT),
        );
        ui.add_space(8.0);
        let mut b = app.brightness;
        let w = ui.available_width();
        if widgets::slider(ui, egui::Id::new("bright"), &mut b, 0.0, 100.0, 1.0, w, ACCENT)
            .drag_stopped()
        {
            app.backend.req(Req::SetBrightness(b as u8));
        }
        app.brightness = b;
    });
    ui.add_space(12.0);
    theme::card(ui, CardStyle::new().pad(20.0), |ui| {
        ui.label(
            egui::RichText::new("静态颜色")
                .font(app.ty.h2.clone())
                .color(TEXT),
        );
        ui.add_space(8.0);
        let colors: [[u8; 3]; 8] = [
            [0x44, 0xD6, 0x2C],
            [0x22, 0xC4, 0xFF],
            [0xB4, 0x4C, 0xFF],
            [0xFF, 0x3D, 0x7F],
            [0xFF, 0xB0, 0x2E],
            [0xFF, 0xFF, 0xFF],
            [0x1E, 0x88, 0xFF],
            [0xFF, 0x44, 0x44],
        ];
        ui.horizontal(|ui| {
            for c in colors {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(38.0), Sense::click());
                let col = Color32::from_rgb(c[0], c[1], c[2]);
                let sel = app.light_color == c;
                ui.painter().rect_filled(rect, Rounding::same(9.0), col);
                if sel {
                    ui.painter().rect_stroke(
                        rect.expand(3.0),
                        Rounding::same(11.0),
                        Stroke::new(1.5, alpha(col, 180)),
                    );
                }
                if resp.clicked() {
                    app.light_color = c;
                    app.backend.req(Req::SetColor(c));
                }
            }
        });
    });
}

// ---------------- 设置 ----------------
pub fn settings(app: &mut App, ui: &mut Ui) {
    page_head(app, ui, "设置", "设备信息、系统集成与关于");
    let st = app.state.clone().unwrap_or_default();

    ui.columns(2, |cols| {
        theme::card(&mut cols[0], CardStyle::new(), |ui| {
            ui.label(
                egui::RichText::new("设备")
                    .font(app.ty.h2.clone())
                    .color(TEXT),
            );
            ui.add_space(8.0);
            theme::key_value(ui, &app.ty, "型号", if st.name.is_empty() { "—" } else { &st.name });
            theme::key_value(ui, &app.ty, "VID:PID", &format!("1532:{:04X}", st.pid));
            theme::key_value(ui, &app.ty, "固件", if st.firmware.is_empty() { "—" } else { &st.firmware });
            theme::key_value(ui, &app.ty, "序列号", if st.serial.is_empty() { "—" } else { &st.serial });
            theme::key_value(ui, &app.ty, "最大 DPI", &format!("{}", st.max_dpi));
            theme::key_value(ui, &app.ty, "灯区", if st.has_rgb { "支持" } else { "无" });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if widgets::ghost_button(ui, &app.ty, "重新连接").clicked() {
                    app.backend.req(Req::Reconnect);
                }
                if widgets::ghost_button(ui, &app.ty, "刷新状态").clicked() {
                    app.backend.req(Req::Refresh);
                }
            });
        });

        theme::card(&mut cols[1], CardStyle::new(), |ui| {
            ui.label(
                egui::RichText::new("系统")
                    .font(app.ty.h2.clone())
                    .color(TEXT),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let mut auto = app.cfg.launch_on_start;
                if widgets::toggle(ui, egui::Id::new("autostart"), &mut auto).changed() {
                    app.cfg.launch_on_start = auto;
                    crate::system::set_autostart(auto);
                    let _ = app.cfg.save();
                }
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("开机自动启动")
                            .font(app.ty.body.clone())
                            .color(TEXT),
                    );
                    ui.label(
                        egui::RichText::new("写入 HKCU\\…\\Run")
                            .font(app.ty.tiny.clone())
                            .color(MUTED),
                    );
                });
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let mut sync = app.cfg.sync_xy;
                if widgets::toggle(ui, egui::Id::new("sync_xy2"), &mut sync).changed() {
                    app.cfg.sync_xy = sync;
                    let _ = app.cfg.save();
                }
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("X/Y 灵敏度同步")
                            .font(app.ty.body.clone())
                            .color(TEXT),
                    );
                    ui.label(
                        egui::RichText::new("编辑阶梯时同时写入两轴")
                            .font(app.ty.tiny.clone())
                            .color(MUTED),
                    );
                });
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if widgets::ghost_button(ui, &app.ty, "打开配置目录").clicked() {
                    crate::system::open_config_dir();
                }
                if widgets::ghost_button(ui, &app.ty, "重置映射").clicked() {
                    app.cfg.remaps = [Action::Pass; 5];
                    app.cfg.remap_enabled = false;
                    let _ = app.cfg.save();
                    app.sync_hook();
                }
            });
        });
    });

    ui.add_space(12.0);

    theme::card(ui, CardStyle::new().pad(20.0), |ui| {
        ui.horizontal(|ui| {
            let (logo_rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
            art::logo(ui.painter(), logo_rect, ACCENT);
            ui.add_space(10.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("RazerControl 1.0.0")
                        .font(app.ty.h1.clone())
                        .color(TEXT),
                );
                ui.label(
                    egui::RichText::new(
                        "纯 Rust 原生驱动。协议由 RazerAppEngine 4.0.699 (Synapse 4) 逆向得到：\
                         HID 控制接口、91 字节 Feature Report、XOR 校验与 XOR 状态机。",
                    )
                    .font(app.ty.small.clone())
                    .color(MUTED),
                );
            });
        });
        ui.add_space(8.0);
        divider(ui);
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "不依赖 Synapse，不注入进程，不安装内核驱动。所有配置直接写入设备或本地 JSON。",
            )
            .font(app.ty.tiny.clone())
            .color(MUTED),
        );
    });
}
