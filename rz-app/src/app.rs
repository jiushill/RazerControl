//! RazerControl —— 原生 Windows 桌面驱动面板（egui 自绘，无 Web 界面）。

use crate::art;
use crate::backend::{Backend, Evt, Req};
use crate::cache;
use crate::config::Config;
use crate::hook;
use crate::pages;
use crate::photo;
use crate::tray;
use crate::theme::{self, alpha, Type, ACCENT, BG, BORDER, CARD, DANGER, DIM, INFO, MUTED, SIDEBAR, TEXT, TRACK, WARN};
use crate::widgets;
use egui::{Align, Align2, Color32, Context, FontId, Id, Pos2, Rect, Rounding, Sense, Stroke, Vec2};
use rz_core::DeviceState;
use std::time::Duration;

pub const PAGE_NAMES: [&str; 8] = ["概览", "DPI", "性能", "校准", "按键", "宏", "灯效", "设置"];

struct Toast {
    msg: String,
    ok: bool,
    born: f64,
}

pub struct App {
    pub ty: Type,
    pub backend: Backend,
    pub state: Option<DeviceState>,
    pub photo_dav3_black: Option<photo::Photo>,
    pub photo_dav3_white: Option<photo::Photo>,
    pub photo_dav2_black: Option<photo::Photo>,
    pub cfg: Config,
    /// 已保存的宏
    pub macros: Vec<crate::macros::Macro>,
    /// 是否正在录制（与 macros::is_recording 同步）
    pub macro_recording: bool,
    /// 回放速度
    pub macro_speed: f32,
    /// 当前展开查看事件的宏
    pub macro_sel: Option<usize>,
    /// 录制时是否记录鼠标移动
    pub macro_track_moves: bool,
    /// 正在等待用户按快捷键
    pub macro_capture: bool,
    /// 快捷键的目标宏（None = 录制用快捷键）
    pub macro_capture_idx: Option<usize>,
    /// 录制前设置的快捷键
    pub macro_record_hotkey: u8,
    /// 录制快捷键需要的修饰键（MOD_* 掩码）
    pub macro_record_mods: u8,
    /// 录制启动延迟（毫秒）
    pub macro_start_delay_ms: u32,
    /// 等待按一个键来填「键盘功能」步骤
    pub macro_add_key: bool,
    /// 待填充的按键步骤下标
    pub macro_target_step: Option<usize>,
    /// 正在编辑的文本步骤（步骤下标, 草稿）
    pub text_edit: Option<(usize, String)>,
    /// 正在编辑的启动步骤（步骤下标, 类型, 目标）
    pub launch_edit: Option<(usize, i32, String)>,
    /// 撤销/重做栈（按当前宏的步骤快照）
    pub macro_undo: Vec<Vec<crate::macros::Ev>>,
    pub macro_redo: Vec<Vec<crate::macros::Ev>>,
    pub page: usize,
    toasts: Vec<Toast>,
    pub connected: bool,
    pub asleep: bool,
    pub dpi_draft: Vec<f32>,
    pub hover_btn: Option<usize>,
    pub sel_btn: usize,
    pub maximized: bool,
    boot_time: f64,
    pub light_color: [u8; 3],
    pub light_effect: usize,
    pub brightness: f32,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = &cc.egui_ctx;
        let ty = theme::install(ctx);
        let cfg = Config::load();
        let backend = Backend::spawn(ctx.clone());
        backend.req(Req::Refresh);
        hook::install();
        tray::install(ctx.clone());
        tray::hook_close();
        hook::configure(cfg.remap_enabled, cfg.remaps, backend.sender());
        hook::set_macros(crate::macros::load_all());
        let photo_dav3_black = photo::load(
            ctx,
            include_bytes!("../assets/device_dav3pro.png"),
            "device_dav3pro_black",
            photo::CROP_DAV3PRO,
            photo::HOTSPOTS_DAV3PRO,
        );
        let photo_dav3_white = photo::load(
            ctx,
            include_bytes!("../assets/device_dav3pro_white.png"),
            "device_dav3pro_white",
            photo::CROP_DAV3PRO,
            photo::HOTSPOTS_DAV3PRO,
        );
        let photo_dav2_black = photo::load(
            ctx,
            include_bytes!("../assets/device_dav2pro.png"),
            "device_dav2pro_black",
            photo::CROP_DAV2PRO,
            photo::HOTSPOTS_DAV2PRO,
        );
        App {
            ty,
            backend,
            state: cache::load(),
            photo_dav3_black,
            photo_dav3_white,
            photo_dav2_black,
            macros: crate::macros::load_all(),
            macro_recording: false,
            macro_speed: 1.0,
            macro_sel: None,
            macro_track_moves: false,
            macro_capture: false,
            macro_capture_idx: None,
            macro_record_hotkey: 0,
            macro_record_mods: 0,
            macro_start_delay_ms: 0,
            macro_add_key: false,
            macro_target_step: None,
            text_edit: None,
            launch_edit: None,
            macro_undo: Vec::new(),
            macro_redo: Vec::new(),
            page: cfg.last_page.min(7),
            toasts: Vec::new(),
            connected: false,
            asleep: false,
            dpi_draft: Vec::new(),
            hover_btn: None,
            sel_btn: art::BTN_BACK,
            maximized: false,
            boot_time: ctx.input(|i| i.time),
            light_color: [0x44, 0xD6, 0x2C],
            light_effect: 0,
            brightness: 80.0,
            cfg,
        }
    }

    /// 记录一次撤销点（改写步骤前调用）
    pub fn push_macro_undo(&mut self, i: usize) {
        if let Some(m) = self.macros.get(i) {
            self.macro_undo.push(m.events.clone());
            if self.macro_undo.len() > 64 {
                self.macro_undo.remove(0);
            }
            self.macro_redo.clear();
        }
    }

    /// 宏落盘 + 同步给钩子
    pub fn save_macros(&mut self) {
        let _ = crate::macros::save_all(&self.macros);
        crate::hook::set_macros(self.macros.clone());
    }

    pub fn toast(&mut self, msg: String, ok: bool, now: f64) {
        self.toasts.push(Toast {
            msg,
            ok,
            born: now,
        });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    fn pump(&mut self, ctx: &Context) {
        let now = ctx.input(|i| i.time);
        for e in self.backend.drain() {
            match e {
                Evt::State(st) => {
                    let st = merge(self.state.as_ref(), *st);
                    if !st.profile.stages.is_empty() {
                        cache::save(&st);
                    }
                    if self.dpi_draft.len() != st.profile.stages.len() {
                        self.dpi_draft = st.profile.stages.iter().map(|s| s.x as f32).collect();
                    }
                    if let Some(b) = st.brightness {
                        self.brightness = b as f32;
                    }
                    self.state = Some(st);
                    self.connected = true;
                }
                Evt::Quick(st) => {
                    // 轻量轮询只更新这些字段；其它信息（固件/序列号/亮度/配色/校准值）
                    // 来自完整快照，不能被 1s 轮询清空。
                    let st = *st;
                    if let Some(cur) = self.state.as_mut() {
                        if st.dpi != (0, 0) {
                            cur.dpi = st.dpi;
                        }
                        if !st.profile.stages.is_empty() {
                            cur.profile = st.profile;
                        }
                        if st.battery.present {
                            cur.battery = st.battery;
                        }
                        if cur.name.is_empty() {
                            cur.name = st.name;
                        }
                        if cur.pid == 0 {
                            cur.pid = st.pid;
                        }
                        if st.wireless {
                            cur.wireless = true;
                        }
                        if st.max_dpi != 0 {
                            cur.max_dpi = st.max_dpi;
                        }
                        if st.max_poll != 0 {
                            cur.max_poll = st.max_poll;
                        }
                        if st.has_rgb {
                            cur.has_rgb = true;
                        }
                        cur.caps = st.caps;
                        if self.dpi_draft.len() != cur.profile.stages.len() {
                            self.dpi_draft =
                                cur.profile.stages.iter().map(|s| s.x as f32).collect();
                        }
                    } else {
                        self.dpi_draft =
                            st.profile.stages.iter().map(|s| s.x as f32).collect();
                        self.state = Some(st);
                    }
                    self.connected = true;
                    self.asleep = false;
                }
                Evt::Toast(msg, ok) => self.toast(msg, ok, now),
                Evt::Sleeping(v) => {
                    if v && !self.asleep {
                        // 低功耗是设备固件的正常行为（空闲约 60s），不当成错误提示
        let _ = now;
                    }
                    self.asleep = v;
                }
                Evt::Lost(msg) => {
                    if self.connected || self.state.is_none() {
                        self.connected = false;
                        self.toast(msg, false, now);
                    }
                }
            }
        }
        self.connected = self.backend.connected.load(std::sync::atomic::Ordering::Relaxed);
    }

    /// 关闭按钮：收进托盘而不是退出
    fn hide_to_tray(&mut self, ctx: &Context) {
        let _ = self.cfg.save();
        tray::hide_main();
        tray::notify("RazerControl", "已最小化到托盘 · 右键托盘图标可退出");
        let _ = ctx;
    }

    /// 结束录制并把结果存进列表
    /// 把录好的事件落成一个宏（按钮停止与热键停止共用）
    pub fn finish_recording(&mut self, events: Vec<crate::macros::Ev>, origin: (i32, i32)) {
        if events.is_empty() {
            return;
        }
        let name = format!("宏 {}", self.macros.len() + 1);
        let mut profile = crate::macros::Profile::default();
        profile.start_delay_ms = self.macro_start_delay_ms;
        profile.move_mode = if self.macro_track_moves {
            crate::macros::MoveMode::Relative
        } else {
            crate::macros::MoveMode::None
        };
        self.macros.push(crate::macros::Macro {
            name: name.clone(),
            events,
            origin,
            profile,
        });
        self.save_macros();
        self.macro_sel = Some(self.macros.len() - 1);
    }

    pub fn macro_finish(&mut self, now: f64) {
        self.macro_recording = false;
        if let Some((events, origin)) = crate::macros::stop_recording() {
            if events.is_empty() {
                self.toast("没有录到任何事件".into(), false, now);
                return;
            }
            let name = format!("宏 {}", self.macros.len() + 1);
            let mut profile = crate::macros::Profile::default();
            // 录制前设的快捷键和启动延迟要写进新宏，否则 hotkey_map 过滤掉 0，
            // 这个宏永远不会被绑定 —— 表现就是"设了快捷键但按了没反应"。
            profile.shortcut = self.macro_record_hotkey;
            profile.start_delay_ms = self.macro_start_delay_ms;
            profile.move_mode = if self.macro_track_moves {
                crate::macros::MoveMode::Relative
            } else {
                crate::macros::MoveMode::None
            };
            self.macros.push(crate::macros::Macro {
                name: name.clone(),
                events,
                origin,
                profile,
            });
            crate::hook::set_macros(self.macros.clone());
            match crate::macros::save_all(&self.macros) {
                Ok(_) => {
                    let n = self.macros.len();
                    self.toast(format!("已保存「{}」（共 {} 个宏）", name, n), true, now)
                }
                Err(e) => self.toast(format!("保存失败: {}", e), false, now),
            }
        }
    }

    /// 当前型号 + 配色对应的官方产品图（editionId 0x80 = 白色）
    pub fn photo(&self) -> Option<&photo::Photo> {
        let st = self.state.as_ref()?;
        let white = st.edition == 0x80;
        let (black, white_photo) = match st.pid {
            // DeathAdder V2 Pro 目前只有官方黑色渲染；白色版先回退黑色，避免用错素材
            0x007C | 0x007D => (&self.photo_dav2_black, &self.photo_dav2_black),
            _ => (&self.photo_dav3_black, &self.photo_dav3_white),
        };
        if white {
            white_photo.as_ref().or(black.as_ref())
        } else {
            black.as_ref().or(white_photo.as_ref())
        }
    }

    pub fn sync_hook(&mut self) {
        hook::configure(self.cfg.remap_enabled, self.cfg.remaps, self.backend.sender());
    }

    // ---------------- 标题栏 ----------------
    fn titlebar(&mut self, ctx: &Context) {
        egui::TopBottomPanel::top("titlebar")
            .exact_height(46.0)
            .frame(egui::Frame::none().fill(SIDEBAR))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                ui.painter().rect_filled(rect, 0.0, SIDEBAR);
                ui.painter().line_segment(
                    [
                        Pos2::new(rect.left(), rect.bottom() - 0.5),
                        Pos2::new(rect.right(), rect.bottom() - 0.5),
                    ],
                    Stroke::new(1.0, BORDER),
                );

                let drag_rect = Rect::from_min_max(
                    rect.min,
                    Pos2::new(rect.right() - 150.0, rect.bottom()),
                );
                let resp = ui.interact(drag_rect, Id::new("tb_drag"), Sense::click_and_drag());
                if resp.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if resp.double_clicked() {
                    self.maximized = !self.maximized;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.maximized));
                }

                let logo_rect = Rect::from_min_size(
                    Pos2::new(rect.left() + 14.0, rect.center().y - 11.0),
                    Vec2::splat(22.0),
                );
                art::logo(ui.painter(), logo_rect, ACCENT);
                ui.painter().text(
                    Pos2::new(logo_rect.right() + 10.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    "RAZER CONTROL",
                    FontId::new(12.5, self.ty.semi.clone()),
                    TEXT,
                );
                ui.painter().text(
                    Pos2::new(logo_rect.right() + 10.0 + 118.0, rect.center().y + 1.0),
                    Align2::LEFT_CENTER,
                    "native · rust",
                    FontId::new(10.0, egui::FontFamily::Proportional),
                    MUTED,
                );

                // 右侧窗口按钮
                let mut x = rect.right() - 8.0;
                for (icon_kind, danger) in [
                    (widgets::Icon::Close, true),
                    (widgets::Icon::Max, false),
                    (widgets::Icon::Min, false),
                ] {
                    let br = Rect::from_min_size(
                        Pos2::new(x - 42.0, rect.center().y - 15.0),
                        Vec2::new(42.0, 30.0),
                    );
                    let r = ui.interact(br, Id::new(("wb", icon_kind as usize)), Sense::click());
                    if r.hovered() {
                        ui.painter().rect_filled(
                            br,
                            Rounding::same(7.0),
                            if danger {
                                Color32::from_rgb(0xC4, 0x2B, 0x2B)
                            } else {
                                alpha(Color32::WHITE, 14)
                            },
                        );
                    }
                    let col = if r.hovered() && danger {
                        Color32::WHITE
                    } else if r.hovered() {
                        TEXT
                    } else {
                        DIM
                    };
                    widgets::icon(
                        ui.painter(),
                        Rect::from_center_size(br.center(), Vec2::splat(14.0)),
                        icon_kind,
                        col,
                    );
                    if r.clicked() {
                        match icon_kind {
                            widgets::Icon::Close => self.hide_to_tray(ctx),
                            widgets::Icon::Max => {
                                self.maximized = !self.maximized;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(
                                    self.maximized,
                                ));
                            }
                            _ => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                        }
                    }
                    x -= 44.0;
                }
            });
    }

    // ---------------- 侧栏 ----------------
    fn sidebar(&mut self, ctx: &Context) {
        egui::SidePanel::left("nav")
            .exact_width(214.0)
            .resizable(false)
            .frame(egui::Frame::none().fill(SIDEBAR))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                ui.painter().rect_filled(rect, 0.0, SIDEBAR);
                ui.painter().line_segment(
                    [
                        Pos2::new(rect.right() - 0.5, rect.top()),
                        Pos2::new(rect.right() - 0.5, rect.bottom()),
                    ],
                    Stroke::new(1.0, BORDER),
                );
                ui.add_space(14.0);
                let icons = [
                    widgets::Icon::Dashboard,
                    widgets::Icon::Dpi,
                    widgets::Icon::Perf,
                    widgets::Icon::Calib,
                    widgets::Icon::Buttons,
                    widgets::Icon::Macro,
                    widgets::Icon::Light,
                    widgets::Icon::Settings,
                ];
                let w = rect.width() - 24.0;
                for (i, name) in PAGE_NAMES.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        if widgets::nav_item(ui, &self.ty, icons[i], name, self.page == i, w).clicked()
                        {
                            self.page = i;
                            self.cfg.last_page = i;
                            let _ = self.cfg.save();
                        }
                    });
                    ui.add_space(2.0);
                }

                // 底部设备状态（直接绘制，避免布局偏移）
                let fw = rect.width() - 24.0;
                let card = Rect::from_min_size(
                    Pos2::new(rect.left() + 12.0, rect.bottom() - 78.0),
                    Vec2::new(fw, 60.0),
                );
                let p = ui.painter();
                p.rect_filled(card, Rounding::same(11.0), CARD);
                p.rect_stroke(card, Rounding::same(11.0), Stroke::new(1.0, BORDER));
                let dot = Pos2::new(card.left() + 15.0, card.top() + 19.0);
                let col = if self.asleep {
                    WARN
                } else if self.connected {
                    ACCENT
                } else {
                    MUTED
                };
                if self.connected || self.asleep {
                    p.circle_filled(dot, 6.5, alpha(col, 40));
                }
                p.circle_filled(dot, 3.5, col);
                let name = self
                    .state
                    .as_ref()
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| "未连接".into());
                p.text(
                    Pos2::new(dot.x + 12.0, dot.y),
                    Align2::LEFT_CENTER,
                    clip_text(&name, 17),
                    FontId::new(12.0, self.ty.semi.clone()),
                    if self.connected { TEXT } else { DIM },
                );
                let sub = if self.asleep {
                    "低功耗 · 移动鼠标刷新".to_string()
                } else if let Some(st) = self.state.as_ref() {
                    if st.caps.battery {
                        format!(
                            "{}% · {}",
                            st.battery.percent,
                            if st.battery.charging { "充电中" } else { "电池" }
                        )
                    } else if self.connected {
                        "已连接 · USB".to_string()
                    } else {
                        "等待设备".to_string()
                    }
                } else {
                    "等待设备".to_string()
                };
                p.text(
                    Pos2::new(dot.x + 12.0, dot.y + 17.0),
                    Align2::LEFT_CENTER,
                    sub,
                    FontId::new(10.5, egui::FontFamily::Proportional),
                    MUTED,
                );
                if let Some(st) = self.state.as_ref() {
                    if self.connected && st.caps.battery {
                        let bar = Rect::from_min_size(
                            Pos2::new(card.left() + 15.0, card.bottom() - 13.0),
                            Vec2::new(card.width() - 30.0, 3.0),
                        );
                        p.rect_filled(bar, Rounding::same(2.0), TRACK);
                        let f = (st.battery.percent as f32 / 100.0).clamp(0.0, 1.0);
                        p.rect_filled(
                            Rect::from_min_size(bar.min, Vec2::new(bar.width() * f, bar.height())),
                            Rounding::same(2.0),
                            if st.battery.percent < 15 { WARN } else { ACCENT },
                        );
                    }
                }
                p.text(
                    Pos2::new(rect.left() + 16.0, rect.bottom() - 92.0),
                    Align2::LEFT_CENTER,
                    "逆向协议 · 免 Synapse",
                    FontId::new(10.0, egui::FontFamily::Proportional),
                    MUTED,
                );
            });
    }

    // ---------------- 内容区 ----------------
    fn content(&mut self, ctx: &Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                ui.painter().rect_filled(rect, 0.0, BG);
                // 顶部细腻的冷光渐变（条带插值，避免椭圆填充的硬边）
                let steps = 26;
                let band_h = 210.0;
                for i in 0..steps {
                    let t = i as f32 / steps as f32;
                    let y = rect.top() + band_h * t;
                    let a = ((1.0 - t).powf(2.2) * 7.0) as u8;
                    if a == 0 {
                        continue;
                    }
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            Pos2::new(rect.left(), y),
                            Vec2::new(rect.width(), band_h / steps as f32 + 1.0),
                        ),
                        0.0,
                        alpha(Color32::from_rgb(0x2E, 0x6B, 0x3A), a),
                    );
                }

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(22.0);
                        ui.horizontal(|ui| {
                            ui.add_space(26.0);
                            let w = ui.available_width() - 26.0;
                            ui.vertical(|ui| {
                                ui.set_width(w);
                                match self.page {
                                    0 => pages::dashboard(self, ui),
                                    1 => pages::dpi(self, ui),
                                    2 => pages::performance(self, ui),
                                    3 => pages::calibration(self, ui),
                                    4 => pages::buttons(self, ui),
                                    5 => pages::macros(self, ui),
                                    6 => pages::lighting(self, ui),
                                    _ => pages::settings(self, ui),
                                }
                                ui.add_space(26.0);
                            });
                        });
                    });

                // 右下角缩放热区
                let grip = Rect::from_min_size(
                    Pos2::new(rect.right() - 16.0, rect.bottom() - 16.0),
                    Vec2::splat(16.0),
                );
                let gr = ui.interact(grip, Id::new("resize_grip"), Sense::drag());
                if gr.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(
                        egui::viewport::ResizeDirection::SouthEast,
                    ));
                }
            });
    }

    fn toasts(&mut self, ctx: &Context) {
        let now = ctx.input(|i| i.time);
        self.toasts.retain(|t| now - t.born < 3.2);
        let n = self.toasts.len();
        for (i, t) in self.toasts.iter().enumerate() {
            let age = (now - t.born) as f32;
            let fade = if age > 2.6 {
                (1.0 - (age - 2.6) / 0.6).clamp(0.0, 1.0)
            } else {
                (age / 0.18).clamp(0.0, 1.0)
            };
            let screen = ctx.screen_rect();
            let w = 268.0;
            let h = 44.0;
            let pos = Pos2::new(
                screen.right() - w - 18.0,
                screen.bottom() - 18.0 - (n - i) as f32 * (h + 8.0),
            );
            let layer = egui::LayerId::new(egui::Order::Tooltip, Id::new("toast").with(i));
            let p = ctx.layer_painter(layer);
            let rect = Rect::from_min_size(
                pos + Vec2::new(0.0, (1.0 - fade) * 10.0),
                Vec2::new(w, h),
            );
            let col = if t.ok { ACCENT } else { DANGER };
            p.rect_filled(
                rect,
                Rounding::same(10.0),
                Color32::from_rgba_unmultiplied(0x12, 0x18, 0x15, (245.0 * fade) as u8),
            );
            p.rect_stroke(
                rect,
                Rounding::same(10.0),
                Stroke::new(1.0, alpha(col, (140.0 * fade) as u8)),
            );
            p.circle_filled(
                Pos2::new(rect.left() + 18.0, rect.center().y),
                4.0,
                alpha(col, (255.0 * fade) as u8),
            );
            p.text(
                Pos2::new(rect.left() + 32.0, rect.center().y),
                Align2::LEFT_CENTER,
                &t.msg,
                FontId::new(12.0, egui::FontFamily::Proportional),
                alpha(TEXT, (255.0 * fade) as u8),
            );
        }
        if !self.toasts.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}

fn merge(old: Option<&DeviceState>, mut new: DeviceState) -> DeviceState {
    // 设备有时会"半答半不答"：身份信息（名称/固件/版本号）读得到，
    // 但 DPI / 轮询率 / 电量全部超时。这种残缺快照绝不能覆盖已有数值，
    // 否则界面会卡在「状态未知 · — · 无数据」直到重启。
    if let Some(o) = old {
        if new.dpi == (0, 0) {
            new.dpi = o.dpi;
        }
        if new.poll_hz == 0 {
            new.poll_hz = o.poll_hz;
            new.poll_code = o.poll_code;
        }
        if !new.battery.present {
            new.battery = o.battery;
        }
        if new.max_dpi == 0 {
            new.max_dpi = o.max_dpi;
            // spec 没读到 → 连接方式也不可信（否则会误显示成 USB）
            new.wireless = o.wireless;
        }
        if new.max_poll == 0 {
            new.max_poll = o.max_poll;
        }
        if !new.has_rgb {
            new.has_rgb = o.has_rgb;
        }
        if new.firmware.is_empty() {
            new.firmware = o.firmware.clone();
        }
        if new.serial.is_empty() {
            new.serial = o.serial.clone();
        }
        if new.edition == 0 {
            new.edition = o.edition;
        }
        if !new.caps.dpi && !new.caps.poll_rate && !new.caps.battery {
            new.caps = o.caps;
        }
        if new.profile.stages.is_empty() {
            new.profile = o.profile.clone();
            new.sleep_secs = o.sleep_secs;
            new.low_battery_percent = o.low_battery_percent;
            new.brightness = new.brightness.or(o.brightness);
        }
        if new.name.is_empty() {
            new.name = o.name.clone();
        }
    }
    new
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.027, 0.035, 0.039, 1.0]
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.cfg.save();
    }

    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // 窗口有焦点时按键可能到不了全局钩子 —— 录制热键在这里兜底
        if self.macro_record_hotkey != 0 && !crate::macros::is_playing() {
            let key = self.macro_record_hotkey;
            let need_mods = self.macro_record_mods;
            let (hit, focused_now, keys_seen) = ctx.input(|i| {
                let mut hit = false;
                let mut seen: Vec<u8> = Vec::new();
                for e in &i.events {
                    if let egui::Event::Key { key: k, pressed: true, .. } = e {
                        if let Some(vk) = crate::macros::egui_key_vk(*k) {
                            seen.push(vk);
                            if vk == key {
                                hit = true;
                            }
                        }
                    }
                }
                let mut mods = 0u8;
                if i.modifiers.ctrl {
                    mods |= crate::macros::MOD_CTRL;
                }
                if i.modifiers.shift {
                    mods |= crate::macros::MOD_SHIFT;
                }
                if i.modifiers.alt {
                    mods |= crate::macros::MOD_ALT;
                }
                if i.modifiers.mac_cmd || i.modifiers.command {
                    mods |= crate::macros::MOD_WIN;
                }
                (hit && mods == need_mods, i.focused, seen)
            });
            if !keys_seen.is_empty() {
                crate::hook::debug_log(&format!(
                    "update keys={:02X?} focused={} rec_key=0x{key:02X} hit={}",
                    keys_seen, focused_now, hit
                ));
            }
            if hit {
                if crate::macros::is_recording() {
                    if let Some(r) = crate::macros::stop_recording() {
                        self.macro_recording = false;
                        let (events, origin) = r;
                        self.finish_recording(events, origin);
                        let n = self.macros.len();
                        let now = ctx.input(|i| i.time);
                        self.toast(format!("已保存第 {} 个宏", n), true, now);
                    }
                } else {
                    crate::macros::start_recording(self.macro_track_moves);
                    self.macro_recording = true;
                }
            }
        }

        // 热键录制结束：把结果落成宏
        if let Some((events, origin)) = crate::macros::take_finished() {
            self.macro_recording = false;
            self.finish_recording(events, origin);
        }
        if crate::macros::is_recording() && !self.macro_recording {
            self.macro_recording = true;
        }
        // 把窗口焦点/可见性告诉后台线程（决定轮询频率）
        let focused = ctx.input(|i| i.focused);
        crate::backend::set_ui_state(focused, true);
        self.pump(ctx);
        if ctx.input(|i| i.time) - self.boot_time < 0.6 {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        ctx.request_repaint_after(Duration::from_millis(500));
        self.titlebar(ctx);
        self.sidebar(ctx);
        self.content(ctx);
        self.toasts(ctx);
    }
}

/// 简易按字符截断（中英文混排够用）
pub fn clip_text(s: &str, max: usize) -> String {
    let mut out = String::new();
    let mut w = 0.0f32;
    for ch in s.chars() {
        let cw = if ch.is_ascii() { 0.55 } else { 1.0 };
        if w + cw > max as f32 {
            out.push('…');
            break;
        }
        out.push(ch);
        w += cw;
    }
    out
}
