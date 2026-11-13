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
                // 设备只是进入了低功耗（固件空闲 60s 行为），不是掉线：
                // Synapse 在这种情况下不做任何提示，我们也只用中性色说明。
                ("低功耗 · 移动鼠标刷新", mix(MUTED, TEXT, 0.4))
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
                                .selected_text(match cur {
                                    Action::Macro(k) => match app.macros.get(k as usize) {
                                        Some(m) => format!("宏 · {}", m.name),
                                        None => "宏（已删除）".to_string(),
                                    },
                                    other => other.label().to_string(),
                                })
                                .width(168.0)
                                .show_ui(ui, |ui| {
                                    // 保持默认 / 禁用 放最上面
                                    for a in [Action::Pass, Action::Disabled] {
                                        if ui.selectable_label(a == cur, a.label()).clicked() {
                                            sel = a;
                                        }
                                    }
                                    // 宏紧随其后：菜单常超出窗口底部，放前面才点得到
                                    let n = app.macros.len().min(Action::MAX_MACROS as usize);
                                    if n > 0 {
                                        ui.separator();
                                        for k in 0..n {
                                            let a = Action::Macro(k as u8);
                                            let text = format!("宏 · {}", app.macros[k].name);
                                            if ui.selectable_label(a == cur, text).clicked() {
                                                sel = a;
                                            }
                                        }
                                    }
                                    ui.separator();
                                    for a in Action::ALL.into_iter().skip(2) {
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


/// 从 egui 本帧的输入里取一个刚按下的键（兜底通道）
fn egui_pressed_vk(ui: &Ui) -> Option<u8> {
    ui.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Key { key, pressed: true, .. } => crate::macros::egui_key_vk(*key),
            _ => None,
        })
    })
}

// ---------------- 宏 ----------------
pub fn macros(app: &mut App, ui: &mut Ui) {
    use crate::macros as M;
    use crate::macros::DelayMode;

    page_head(app, ui, "宏", "记录键盘与鼠标动作，数据模型对齐 Razer Synapse");

    // 快捷键捕获（录制前设置 / 编辑器里改绑）
    if app.macro_capture {
        // 捕获期间连续重绘：request_repaint_after 在窗口失焦时会被系统拖后，
        // 导致"按了键要等一会儿才认"。
        ui.ctx().request_repaint();
        let from_hook = crate::hook::take_captured();
        let from_egui = if from_hook.is_none() { egui_pressed_vk(ui) } else { None };
        if let Some(vk) = from_hook.or(from_egui) {
            crate::hook::debug_log(&format!(
                "capture via {} vk=0x{vk:02X}",
                if from_hook.is_some() { "hook" } else { "egui" }
            ));
            app.macro_capture = false;
            if vk == 0x1B {
                // Esc 取消捕获，不占用快捷键
                let now = ui.input(|i| i.time);
                app.toast("已取消".into(), false, now);
                return;
            }
            crate::hook::debug_log(&format!("ui took hotkey vk=0x{vk:02X}"));
            if M::is_modifier(vk) {
                // 单独按修饰键不算热键，继续等
                crate::hook::set_capture(true);
                let now = ui.input(|i| i.time);
                app.toast("请再按一个普通键（如 R、F9）".into(), false, now);
                return;
            }
            match app.macro_capture_idx {
                Some(i) => {
                    if let Some(m) = app.macros.get_mut(i) {
                        m.profile.shortcut = vk;
                    }
                    let _ = M::save_all(&app.macros);
                    crate::hook::set_macros(app.macros.clone());
                }
                None => {
                    // 组合键：记录当时按住的修饰键
                    let mods = {
                        let via_egui = ui.input(|i| {
                            let mut m = 0u8;
                            if i.modifiers.ctrl {
                                m |= M::MOD_CTRL;
                            }
                            if i.modifiers.shift {
                                m |= M::MOD_SHIFT;
                            }
                            if i.modifiers.alt {
                                m |= M::MOD_ALT;
                            }
                            m
                        });
                        if via_egui != 0 {
                            via_egui
                        } else {
                            M::mods_now()
                        }
                    };
                    app.macro_record_hotkey = vk;
                    app.macro_record_mods = mods;
                    crate::hook::set_record_key(vk, mods, app.macro_track_moves);
                }
            }
            let now = ui.input(|i| i.time);
            app.toast(format!("快捷键：{}", M::key_name(vk)), true, now);
        }
    }
    // 「键盘功能」：捕获一个键作为新步骤
    if app.macro_add_key {
        ui.ctx().request_repaint();
        let from_hook2 = crate::hook::take_captured();
        let from_egui2 = if from_hook2.is_none() { egui_pressed_vk(ui) } else { None };
        if let Some(vk) = from_hook2.or(from_egui2) {
            app.macro_add_key = false;
            if let Some(i) = app.macro_sel {
                let sc = M::vk_to_scancode(vk);
                crate::hook::debug_log(&format!(
                    "ui took keypress vk=0x{vk:02X} sc=0x{sc:02X} step={:?}",
                    app.macro_target_step
                ));
                let step = app.macro_target_step.take();
                let mut evs = app.macros[i].events.clone();
                match step {
                    Some(j) if j < evs.len() && evs[j].kind == M::T_KEYPRESS => {
                        app.push_macro_undo(i);
                        evs[j].a = sc as i32;
                    }
                    _ => {
                        app.push_macro_undo(i);
                        evs.push(M::Ev::keypress(sc as i32));
                    }
                }
                app.macros[i].events = evs;
                app.save_macros();
            }
        }
    }
    if app.macro_recording {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(60));
    }

    // ================= 录制条 =================
    theme::card(ui, CardStyle::new().pad(18.0), |ui| {
        ui.horizontal(|ui| {
            let rec = app.macro_recording;
            let live = M::recording_snapshot();
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(if rec { "录制中" } else { "就绪" })
                        .font(app.ty.h2.clone())
                        .color(if rec { WARN } else { TEXT }),
                );
                ui.add_space(2.0);
                let detail = if rec {
                    format!(
                        "已记录 {} 步 · 合计 {:.2} s（下方列表实时显示）",
                        live.len(),
                        M::total_ms(&live) as f32 / 1000.0
                    )
                } else {
                    "点开始录制，之后按下的键和鼠标动作会被实时记录".to_string()
                };
                ui.label(
                    egui::RichText::new(detail)
                        .font(app.ty.small.clone())
                        .color(MUTED),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if rec {
                    if widgets::accent_button(ui, &app.ty, "停止并保存").clicked() {
                        app.macro_finish(ui.input(|i| i.time));
                    }
                } else if widgets::accent_button(ui, &app.ty, "开始录制").clicked() {
                    crate::macros::start_recording(app.macro_track_moves);
                    app.macro_recording = true;
                    app.macro_capture = false;
                    app.macro_add_key = false;
                }
            });
        });

        ui.add_space(10.0);
        divider(ui);
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("启动延迟").font(app.ty.small.clone()).color(MUTED));
            let mut secs = app.macro_start_delay_ms as f64 / 1000.0;
            if ui
                .add(
                    egui::DragValue::new(&mut secs)
                        .speed(0.01)
                        .range(0.0..=60.0)
                        .suffix(" s"),
                )
                .changed()
            {
                app.macro_start_delay_ms = (secs * 1000.0).round().max(0.0) as u32;
            }

            ui.add_space(22.0);
            ui.label(
                egui::RichText::new("录制热键")
                    .font(app.ty.small.clone())
                    .color(MUTED),
            )
            .on_hover_text("按下开始录制，再按一下停止并保存。建议用 F9 或 Ctrl+Alt+R 这类组合键");
            // 录制前就能设好：录完直接生效
            let hot = app.macro_record_hotkey;
            let shown = M::combo_name(hot, app.macro_record_mods);
            if widgets::ghost_button(ui, &app.ty, &shown).clicked() {
                crate::hook::set_capture(true);
                app.macro_capture = true;
                app.macro_capture_idx = None;
            }
            if false && hot != 0 && !app.macro_capture {
                ui.label(
                    egui::RichText::new("（还没有宏，按这个键不会有动作）")
                        .font(app.ty.tiny.clone())
                        .color(WARN),
                );
            } else if app.macro_capture && app.macro_capture_idx.is_none() {
                ui.label(
                    egui::RichText::new("请按键…（Esc 取消）")
                        .font(app.ty.tiny.clone())
                        .color(WARN),
                );
            } else if hot != 0 {
                if ui
                    .small_button("✕")
                    .on_hover_text("清除快捷键")
                    .clicked()
                {
                    app.macro_record_hotkey = 0;
                    app.macro_record_mods = 0;
                    crate::hook::set_record_key(0, 0, app.macro_track_moves);
                }
            }

            ui.add_space(22.0);
            ui.checkbox(&mut app.macro_track_moves, "录制鼠标移动");
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("回放速度").font(app.ty.small.clone()).color(MUTED));
            let mut sp = app.macro_speed;
            widgets::slider(
                ui,
                egui::Id::new("macro_speed"),
                &mut sp,
                0.25,
                4.0,
                0.25,
                180.0,
                ACCENT,
            );
            app.macro_speed = sp;
            ui.label(
                egui::RichText::new(format!("{:.2}×", app.macro_speed))
                    .font(app.ty.mono.clone())
                    .color(TEXT),
            );
        });
    });

    ui.add_space(10.0);

    // ================= 宏选择 =================
    theme::card(ui, CardStyle::new().pad(14.0), |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("宏").font(app.ty.small.clone()).color(MUTED));
            ui.add_space(6.0);
            let mut pick = None;
            for (i, m) in app.macros.iter().enumerate() {
                let on = app.macro_sel == Some(i);
                if widgets::ghost_button(ui, &app.ty, &m.name).clicked() {
                    pick = Some(i);
                }
                if on {
                    ui.label(egui::RichText::new("●").color(ACCENT).font(app.ty.tiny.clone()));
                }
                ui.add_space(4.0);
            }
            if let Some(i) = pick {
                app.macro_sel = Some(i);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::ghost_button(ui, &app.ty, "回放").clicked() {
                    if let Some(i) = app.macro_sel {
                        if let Some(m) = app.macros.get(i).cloned() {
                            crate::macros::play(&m, app.macro_speed);
                        }
                    }
                }
                if widgets::ghost_button(ui, &app.ty, "删除").clicked() {
                    if let Some(i) = app.macro_sel {
                        app.macros.remove(i);
                        app.macro_sel = if app.macros.is_empty() {
                            None
                        } else {
                            Some(i.min(app.macros.len() - 1))
                        };
                        app.save_macros();
                    }
                }
                if widgets::accent_button(ui, &app.ty, "新建").clicked() {
                    let mut p = crate::macros::Profile::default();
                    p.shortcut = app.macro_record_hotkey;
                    p.start_delay_ms = app.macro_start_delay_ms;
                    app.macros.push(crate::macros::Macro {
                        name: crate::macros::unique_name(&app.macros),
                        events: Vec::new(),
                        origin: (0, 0),
                        profile: p,
                    });
                    app.macro_sel = Some(app.macros.len() - 1);
                    app.save_macros();
                }
            });
        });
    });

    ui.add_space(10.0);

    // ================= 编辑器 =================
    let editing = app.macro_recording || app.macro_sel.is_some();
    if !editing {
        theme::card(ui, CardStyle::new().pad(20.0), |ui| {
            ui.label(
                egui::RichText::new("还没有宏")
                    .font(app.ty.h2.clone())
                    .color(TEXT),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!(
                    "点「开始录制」或「新建」。存储位置：{}",
                    crate::macros::store_path()
                ))
                .font(app.ty.small.clone())
                .color(MUTED),
            );
        });
        return;
    }

    let sel = app.macro_sel;
    let live = if app.macro_recording {
        M::recording_snapshot()
    } else {
        Vec::new()
    };

    let mut add_delay = false;
    let mut add_key = false;
    let mut add_mouse = false;
    let mut add_text = false;
    let mut add_launch = false;
    let mut add_run = false;
    let mut add_macro_ref = false;
    let mut add_loop = false;
    let mut undo = false;
    let mut redo = false;
    let mut save = false;

    let (_, _) = theme::card(ui, CardStyle::new().pad(16.0), |ui| {
        // ---- 工具条 ----
        let total = if app.macro_recording {
            M::total_ms(&live)
        } else {
            sel.map(|i| M::total_ms(&app.macros[i].events)).unwrap_or(0)
        };
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{:.3}s", total as f32 / 1000.0))
                    .font(app.ty.mono.clone())
                    .color(TEXT),
            );
            ui.add_space(18.0);

            if let Some(i) = sel {
                let mut mode = app.macros[i].profile.delay_mode;
                let label = match mode {
                    DelayMode::Recorded => "录制延迟",
                    DelayMode::Fixed => "固定延迟",
                    DelayMode::Random => "随机延迟",
                    DelayMode::None => "无延迟",
                };
                ui.menu_button(format!("{} ▾", label), |ui| {
                    if ui.button("录制延迟").clicked() {
                        mode = DelayMode::Recorded;
                        ui.close_menu();
                    }
                    if ui.button("固定延迟").clicked() {
                        mode = DelayMode::Fixed;
                        ui.close_menu();
                    }
                    if ui.button("随机延迟").clicked() {
                        mode = DelayMode::Random;
                        ui.close_menu();
                    }
                    if ui.button("无延迟").clicked() {
                        mode = DelayMode::None;
                        ui.close_menu();
                    }
                });
                if mode != app.macros[i].profile.delay_mode {
                    app.push_macro_undo(i);
                    app.macros[i].profile.delay_mode = mode;
                    app.save_macros();
                }
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::accent_button(ui, &app.ty, "保存").clicked() {
                    save = true;
                }
                ui.add_space(6.0);
                if widgets::ghost_button(ui, &app.ty, "重做").clicked() {
                    redo = true;
                }
                if widgets::ghost_button(ui, &app.ty, "撤销").clicked() {
                    undo = true;
                }
            });
        });

        ui.add_space(8.0);
        divider(ui);
        ui.add_space(8.0);

        ui.horizontal_top(|ui| {
            // ---- 左：添加 ----
            if !app.macro_recording {
                ui.vertical(|ui| {
                    ui.set_width(150.0);
                    ui.label(
                        egui::RichText::new("添加")
                            .font(app.ty.small.clone())
                            .color(MUTED),
                    );
                    ui.add_space(6.0);
                    if ui.button("延迟").clicked() {
                        add_delay = true;
                    }
                    if ui.button("键盘功能").clicked() {
                        add_key = true;
                    }
                    if ui.button("鼠标功能").clicked() {
                        add_mouse = true;
                    }
                    if ui.button("文本功能").clicked() {
                        add_text = true;
                    }
                    if ui.button("启动").clicked() {
                        add_launch = true;
                    }
                    if ui.button("运行命令").clicked() {
                        add_run = true;
                    }
                    if ui.button("宏").clicked() {
                        add_macro_ref = true;
                    }
                    if ui
                        .button("循环")
                        .on_hover_text("把当前所有步骤包进循环（次数可在行内改）")
                        .clicked()
                    {
                        add_loop = true;
                    }
                });
                ui.add_space(10.0);
                let _ = ui.separator();
            }

            // ---- 右：步骤列表 ----
            ui.vertical(|ui| {
                let evs_view = if app.macro_recording {
                    live.clone()
                } else {
                    sel.map(|i| app.macros[i].events.clone()).unwrap_or_default()
                };
                if evs_view.is_empty() {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(if app.macro_recording {
                            "等待你的第一个动作…"
                        } else {
                            "还没有步骤。左边「添加」或直接录制。"
                        })
                        .font(app.ty.small.clone())
                        .color(MUTED),
                    );
                    return;
                }

                let mut changed = false;
                let mut remove: Option<usize> = None;
                let mut evs = evs_view.clone();
                // 长宏（例如开着鼠标移动追踪）只渲染前 400 行，避免每帧卡顿
                let shown = evs.len().min(400);
                if evs.len() > shown {
                    ui.label(
                        egui::RichText::new(format!("共 {} 步，仅显示前 {} 步", evs.len(), shown))
                            .font(app.ty.tiny.clone())
                            .color(MUTED),
                    );
                }
                for j in 0..shown {
                    ui.horizontal(|ui| {
                        if app.macro_recording {
                            // 录制中：只读，显示真实动作
                        } else {
                            let mut on = !M::ev_disabled(&evs[j]);
                            if ui.checkbox(&mut on, "").changed() {
                                M::ev_set_disabled(&mut evs[j], !on);
                                changed = true;
                            }
                        }

                        if evs[j].kind == M::T_DELAY {
                            let mut secs = evs[j].a.max(0) as f64 / 1000.0;
                            ui.add_enabled_ui(!app.macro_recording, |ui| {
                                if ui
                                    .add_sized(
                                        [86.0, 20.0],
                                        egui::DragValue::new(&mut secs)
                                            .speed(0.005)
                                            .range(0.0..=60.0)
                                            .suffix(" s"),
                                    )
                                    .changed()
                                {
                                    evs[j].a = (secs * 1000.0).round() as i32;
                                    changed = true;
                                }
                            });
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new("延迟")
                                    .font(app.ty.tiny.clone())
                                    .color(MUTED),
                            );
                        } else {
                            ui.add_space(90.0);
                            let (badge, col) = match evs[j].kind {
                                M::T_KEYBOARD | M::T_KEYPRESS => ("键", ACCENT),
                                M::T_MOUSE | M::T_ACTION => ("鼠", INFO),
                                M::T_WHEEL => ("轮", TEXT),
                                M::T_TEXT => ("文", ACCENT),
                                M::T_LAUNCH => ("启", ACCENT),
                                M::T_RUN => ("令", INFO),
                                M::T_MACRO => ("M", ACCENT),
                                M::T_LOOP_START | M::T_LOOP_END => ("循", WARN),
                                _ => ("移", MUTED),
                            };
                            let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                            ui.painter()
                                .rect_filled(rect, Rounding::same(4.0), alpha(col, 36));
                            ui.painter().text(
                                rect.center(),
                                Align2::CENTER_CENTER,
                                badge,
                                FontId::proportional(10.0),
                                col,
                            );
                            ui.add_space(6.0);
                            if evs[j].kind == M::T_ACTION && !app.macro_recording {
                                // 雷云交互：行内下拉选择操作
                                let cur = M::action_name(evs[j].a);
                                let unset = evs[j].a < 0;
                                let label = if unset {
                                    "选择操作 ▾".to_string()
                                } else {
                                    format!("{} ▾", cur)
                                };
                                let txt = egui::RichText::new(label)
                                    .font(app.ty.small.clone())
                                    .color(if unset { ACCENT } else { TEXT });
                                ui.menu_button(txt, |ui| {
                                    for (code, (name, en)) in M::ACTIONS.iter().enumerate() {
                                        if ui
                                            .button(*name)
                                            .on_hover_text(*en)
                                            .clicked()
                                        {
                                            evs[j].a = code as i32;
                                            changed = true;
                                            ui.close_menu();
                                        }
                                    }
                                });
                            } else if evs[j].kind == M::T_LOOP_START && !app.macro_recording {
                                ui.label(
                                    egui::RichText::new("循环")
                                        .font(app.ty.small.clone())
                                        .color(MUTED),
                                );
                                let mut times = evs[j].a.max(1);
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut times)
                                            .speed(0.2)
                                            .range(1..=999),
                                    )
                                    .changed()
                                {
                                    evs[j].a = times.max(1);
                                    changed = true;
                                }
                                ui.label(
                                    egui::RichText::new("次（以下步骤重复执行）")
                                        .font(app.ty.tiny.clone())
                                        .color(MUTED),
                                );
                            } else if evs[j].kind == M::T_LOOP_END && !app.macro_recording {
                                ui.label(
                                    egui::RichText::new("循环结束")
                                        .font(app.ty.small.clone())
                                        .color(MUTED),
                                );
                            } else if evs[j].kind == M::T_MACRO && !app.macro_recording {
                                // 雷云：宏 + 选择宏下拉
                                ui.label(
                                    egui::RichText::new("宏：")
                                        .font(app.ty.small.clone())
                                        .color(MUTED),
                                );
                                let cur = evs[j].text.clone();
                                let label = if cur.is_empty() {
                                    "选择宏 ▾".to_string()
                                } else {
                                    format!("{} ▾", cur)
                                };
                                let txt = egui::RichText::new(label)
                                    .font(app.ty.small.clone())
                                    .color(if cur.is_empty() { ACCENT } else { TEXT });
                                ui.menu_button(txt, |ui| {
                                    let names: Vec<String> = app
                                        .macros
                                        .iter()
                                        .map(|m| m.name.clone())
                                        .collect();
                                    let self_name = app
                                        .macro_sel
                                        .and_then(|si| app.macros.get(si))
                                        .map(|m| m.name.clone())
                                        .unwrap_or_default();
                                    let mut any = false;
                                    for n in names {
                                        if n == self_name {
                                            continue; // 不能引用自己
                                        }
                                        any = true;
                                        if ui.button(&n).clicked() {
                                            evs[j].text = n;
                                            changed = true;
                                            ui.close_menu();
                                        }
                                    }
                                    if !any {
                                        ui.label(
                                            egui::RichText::new("没有其它宏可引用")
                                                .color(MUTED),
                                        );
                                    }
                                });
                            } else if evs[j].kind == M::T_RUN && !app.macro_recording {
                                // 雷云是行内直接编辑
                                ui.label(
                                    egui::RichText::new("运行命令：")
                                        .font(app.ty.small.clone())
                                        .color(MUTED),
                                );
                                let mut cmd = evs[j].text.clone();
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut cmd)
                                            .desired_width(320.0)
                                            .font(egui::TextStyle::Monospace)
                                            .hint_text("例如 notepad.exe 或 shutdown /s /t 30"),
                                    )
                                    .changed()
                                {
                                    evs[j].text = cmd;
                                    changed = true;
                                }
                            } else if evs[j].kind == M::T_LAUNCH && !app.macro_recording {
                                let label = if evs[j].text.is_empty() {
                                    "点击设置程序或网站".to_string()
                                } else {
                                    M::ev_label(&evs[j])
                                };
                                let txt = egui::RichText::new(label)
                                    .font(app.ty.small.clone())
                                    .color(if evs[j].text.is_empty() { ACCENT } else { TEXT });
                                if ui.button(txt).on_hover_text("点开设置").clicked() {
                                    let is_url = evs[j].a == M::LAUNCH_URL;
                                    app.launch_edit = Some((
                                        j,
                                        evs[j].a,
                                        if is_url { String::new() } else { evs[j].text.clone() },
                                        if is_url { evs[j].text.clone() } else { String::new() },
                                    ));
                                }
                            } else if evs[j].kind == M::T_TEXT && !app.macro_recording {
                                let label = if evs[j].text.is_empty() {
                                    "点击编辑文本".to_string()
                                } else {
                                    M::ev_label(&evs[j])
                                };
                                let txt = egui::RichText::new(label)
                                    .font(app.ty.small.clone())
                                    .color(if evs[j].text.is_empty() { ACCENT } else { TEXT });
                                if ui.button(txt).on_hover_text("点开编辑这段文字").clicked() {
                                    app.text_edit = Some((j, evs[j].text.clone()));
                                }
                            } else if evs[j].kind == M::T_KEYPRESS && !app.macro_recording {
                                let name = if evs[j].a <= 0 {
                                    "选择键".to_string()
                                } else {
                                    M::sc_name(evs[j].a as u16)
                                };
                                let waiting = app.macro_add_key && app.macro_target_step == Some(j);
                                let label = if waiting { "请按键…" } else { "选择键" };
                                let txt = egui::RichText::new(if waiting {
                                    label.to_string()
                                } else {
                                    name
                                })
                                .font(app.ty.small.clone())
                                .color(if waiting {
                                    WARN
                                } else if evs[j].a <= 0 {
                                    ACCENT
                                } else {
                                    TEXT
                                });
                                if ui
                                    .button(txt)
                                    .on_hover_text("点一下，然后按要用的键")
                                    .clicked()
                                {
                                    app.macro_target_step = Some(j);
                                    app.macro_add_key = true;
                                    crate::hook::set_capture(true);
                                }
                            } else {
                                ui.label(
                                    egui::RichText::new(M::ev_label(&evs[j]))
                                        .font(app.ty.small.clone())
                                        .color(if M::ev_disabled(&evs[j]) { MUTED } else { TEXT }),
                                );
                            }
                        }

                        if !app.macro_recording {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button("✕").on_hover_text("删除这一步").clicked() {
                                    remove = Some(j);
                                }
                            });
                        }
                    });
                }

                if let Some(j) = remove {
                    evs.remove(j);
                    // 合并相邻的两个延迟步骤
                    let mut k = 0;
                    while k + 1 < evs.len() {
                        if evs[k].kind == M::T_DELAY && evs[k + 1].kind == M::T_DELAY {
                            let add = evs[k + 1].a.max(0);
                            evs[k].a = evs[k].a.saturating_add(add);
                            evs.remove(k + 1);
                        } else {
                            k += 1;
                        }
                    }
                    changed = true;
                }

                if changed {
                    if let Some(i) = sel {
                        app.push_macro_undo(i);
                        app.macros[i].events = evs;
                        app.save_macros();
                    }
                }
            });
        });
    });

    // ---- 延迟处理动作 ----
    if let Some(i) = sel {
        if add_delay {
            app.push_macro_undo(i);
            app.macros[i].events.push(M::Ev::delay(100));
            app.save_macros();
        }
        if add_launch {
            app.push_macro_undo(i);
            app.macros[i].events.push(M::Ev::delay(50));
            app.macros[i]
                .events
                .push(M::Ev::launch(M::LAUNCH_PROGRAM, ""));
            let idx = app.macros[i].events.len() - 1;
            app.launch_edit = Some((idx, M::LAUNCH_PROGRAM, String::new(), String::new()));
            app.save_macros();
        }
        if add_run {
            app.push_macro_undo(i);
            app.macros[i].events.push(M::Ev::delay(50));
            app.macros[i].events.push(M::Ev::run(""));
            app.save_macros();
        }
        if add_loop {
            let n = app.macros[i].events.len();
            if n == 0 {
                let now = ui.input(|i| i.time);
                app.toast("先添加步骤，再插入循环".into(), false, now);
            } else {
                app.push_macro_undo(i);
                app.macros[i].events.insert(0, M::Ev::loop_start(2));
                app.macros[i].events.push(M::Ev::loop_end());
                app.save_macros();
            }
        }
        if add_macro_ref {
            app.push_macro_undo(i);
            app.macros[i].events.push(M::Ev::delay(50));
            app.macros[i].events.push(M::Ev::macro_ref(""));
            app.save_macros();
        }
        if add_text {
            app.push_macro_undo(i);
            app.macros[i].events.push(M::Ev::delay(50));
            app.macros[i].events.push(M::Ev::text_step(""));
            let idx = app.macros[i].events.len() - 1;
            app.text_edit = Some((idx, String::new()));
            app.save_macros();
        }
        if add_mouse {
            // 和雷云一样：先插入一个未选择操作的步骤，再在行内下拉里挑动作
            app.push_macro_undo(i);
            app.macros[i].events.push(M::Ev::delay(50));
            app.macros[i]
                .events
                .push(M::Ev::action(M::ACT_UNSET));
            app.save_macros();
        }
    }
    if add_key {
        // 键盘功能：插入一个"选择键"步骤，再按键捕获
        if let Some(i) = sel {
            app.push_macro_undo(i);
            app.macros[i].events.push(M::Ev::delay(50));
            app.macros[i].events.push(M::Ev::keypress(0));
            app.macro_target_step = Some(app.macros[i].events.len() - 1);
            app.save_macros();
        }
        crate::hook::set_capture(true);
        app.macro_add_key = true;
    }
    if undo {
        if let Some(i) = sel {
            if let Some(prev) = app.macro_undo.pop() {
                let cur = app.macros[i].events.clone();
                app.macro_redo.push(cur);
                app.macros[i].events = prev;
                app.save_macros();
            }
        }
    }
    if redo {
        if let Some(i) = sel {
            if let Some(next) = app.macro_redo.pop() {
                let cur = app.macros[i].events.clone();
                app.macro_undo.push(cur);
                app.macros[i].events = next;
                app.save_macros();
            }
        }
    }
    if save {
        let ks: Vec<String> = app.macros.iter().map(|m| m.name.clone()).collect();
        let _ = M::save_all(&app.macros);
        crate::hook::set_macros(app.macros.clone());
        app.toast(format!("已保存 {} 个宏", ks.len()), true, ui.input(|i| i.time));
    }

    // ---------- 文本功能编辑弹窗 ----------
    if let Some((idx, draft0)) = app.text_edit.clone() {
        let mut draft = draft0;
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        egui::Window::new("文本功能")
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .open(&mut open)
            .show(ui.ctx(), |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut draft)
                        .desired_width(320.0)
                        .desired_rows(6)
                        .hint_text("要打出来的文字"),
                );
                let n = draft.chars().count();
                ui.label(
                    egui::RichText::new(format!("{}/{}", n, crate::macros::TEXT_MAX))
                        .font(app.ty.tiny.clone())
                        .color(if n > crate::macros::TEXT_MAX { WARN } else { MUTED }),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if widgets::ghost_button(ui, &app.ty, "取消").clicked() {
                        cancel = true;
                    }
                    if widgets::accent_button(ui, &app.ty, "保存").clicked() {
                        save = true;
                    }
                });
            });

        if save {
            let clipped: String = draft.chars().take(crate::macros::TEXT_MAX).collect();
            if let Some(mi) = app.macro_sel {
                app.push_macro_undo(mi);
                if let Some(e) = app.macros[mi].events.get_mut(idx) {
                    e.text = clipped;
                }
                app.save_macros();
            }
            app.text_edit = None;
        } else if cancel || !open {
            app.text_edit = None;
        } else {
            app.text_edit = Some((idx, draft));
        }
    }

    // ---------- 启动编辑弹窗 ----------
    if let Some((idx, kind0, prog0, url0)) = app.launch_edit.clone() {
        let mut kind = kind0;
        let mut prog = prog0;
        let mut url = url0;
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        let mut browse = false;
        egui::Window::new("启动")
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .open(&mut open)
            .show(ui.ctx(), |ui| {
                ui.radio_value(&mut kind, crate::macros::LAUNCH_PROGRAM, "程序");
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    ui.add_enabled_ui(kind == crate::macros::LAUNCH_PROGRAM, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut prog)
                                .desired_width(300.0)
                                .hint_text("C:\\Path\\To\\App.exe"),
                        );
                        if ui.button("浏览…").clicked() {
                            browse = true;
                        }
                    });
                });
                ui.add_space(6.0);
                ui.radio_value(&mut kind, crate::macros::LAUNCH_URL, "网站");
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    ui.add_enabled_ui(kind == crate::macros::LAUNCH_URL, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut url)
                                .desired_width(340.0)
                                .hint_text("https://example.com"),
                        );
                    });
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if widgets::ghost_button(ui, &app.ty, "取消").clicked() {
                        cancel = true;
                    }
                    if widgets::accent_button(ui, &app.ty, "保存").clicked() {
                        save = true;
                    }
                });
            });

        if browse {
            if let Some(p) = crate::macros::pick_file() {
                prog = p;
                kind = crate::macros::LAUNCH_PROGRAM;
            }
        }

        if save {
            let chosen = if kind == crate::macros::LAUNCH_URL {
                url.trim().to_string()
            } else {
                prog.trim().to_string()
            };
            if let Some(mi) = app.macro_sel {
                app.push_macro_undo(mi);
                if let Some(e) = app.macros[mi].events.get_mut(idx) {
                    e.a = kind;
                    e.text = chosen;
                }
                app.save_macros();
            }
            app.launch_edit = None;
        } else if cancel || !open {
            app.launch_edit = None;
        } else {
            app.launch_edit = Some((idx, kind, prog, url));
        }
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
