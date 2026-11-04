//! 设备后台工作线程：所有 HID I/O 都在这里，UI 线程永不阻塞。

use rz_core::{Cmd, Device, DeviceState, DpiStage};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum Req {
    Refresh,
    SetDpi(u16, u16),
    SetPoll(u16),
    SetSleepMin(u16),
    SetLowBatt(u8),
    SetActiveStage(u8),
    SetStageDpi(usize, u16),
    SetStages(Vec<u16>),
    SetBrightness(u8),
    SetColor([u8; 3]),
    CycleStage,
    SetTracking(u8),
    SetAsymmetric(bool),
    SetLiftLand(u8, u8),
    SetLiftoff(u8),
    Reconnect,
}

#[derive(Debug)]
pub enum Evt {
    /// 完整快照（连接 / 唤醒 / 用户操作后）
    State(Box<DeviceState>),
    /// 轻量轮询：只带 DPI、活动段和（可选的）电量，UI 合并时不覆盖其它字段
    Quick(Box<DeviceState>),
    Toast(String, bool),
    Lost(String),
    /// 设备在但无应答（无线休眠 / 被占用）
    Sleeping(bool),
}

pub struct Backend {
    tx: Sender<Req>,
    rx: Receiver<Evt>,
    pub connected: Arc<AtomicBool>,
}

impl Backend {
    pub fn spawn(ctx: egui::Context) -> Backend {
        let (tx_req, rx_req) = channel::<Req>();
        let (tx_evt, rx_evt) = channel::<Evt>();
        let connected = Arc::new(AtomicBool::new(false));
        let flag = connected.clone();
        thread::spawn(move || worker(ctx, rx_req, tx_evt, flag));
        Backend {
            tx: tx_req,
            rx: rx_evt,
            connected,
        }
    }

    pub fn req(&self, r: Req) {
        let _ = self.tx.send(r);
    }

    pub fn sender(&self) -> Sender<Req> {
        self.tx.clone()
    }

    pub fn drain(&self) -> Vec<Evt> {
        self.rx.try_iter().collect()
    }
}

/// 选出可用的鼠标：跳过 Mouse Dock 等配件，优先有线型号。
fn pick_device() -> Option<Device> {
    rz_core::pick_device()
}

fn worker(ctx: egui::Context, rx: Receiver<Req>, tx: Sender<Evt>, connected: Arc<AtomicBool>) {
    let mut dev: Option<Device> = None;
    let mut last_try = Instant::now() - Duration::from_secs(10);
    let mut last_poll = Instant::now();
    let mut last_battery = Instant::now();
    let mut last_profile_poll = Instant::now();
    let mut last_dpi: Option<(u16, u16)> = None;
    let mut lost_reported = false;
    let mut was_asleep = false;
    let mut fails: u32 = 0;
    let mut lift_land: (u8, u8) = (15, 8);

    loop {
        if dev.is_none() && last_try.elapsed() > Duration::from_millis(1200) {
            last_try = Instant::now();
            match pick_device() {
                Some(mut d) => {
                    let st = d.snapshot();
                    connected.store(true, Ordering::Relaxed);
                    lost_reported = false;
                    let responsive = st.caps.dpi || st.caps.poll_rate || st.caps.battery;
                    if responsive {
                        let _ = tx.send(Evt::Sleeping(false));
                        let _ = tx.send(Evt::State(Box::new(st)));
                    } else {
                        // 接口在、但命令全部超时：无线鼠标休眠或句柄被抢占
                        let _ = tx.send(Evt::Sleeping(true));
                    }
                    dev = Some(d);
                    last_poll = Instant::now();
                    ctx.request_repaint();
                }
                None => {
                    connected.store(false, Ordering::Relaxed);
                    if !lost_reported {
                        lost_reported = true;
                        let _ = tx.send(Evt::Lost("未检测到雷蛇设备".into()));
                        ctx.request_repaint();
                    }
                }
            }
        }

        let req = rx.recv_timeout(Duration::from_millis(220));
        match req {
            Ok(Req::Reconnect) => {
                dev = None;
                last_try = Instant::now() - Duration::from_secs(10);
            }
            Ok(r) => {
                if let Req::SetLiftLand(l, n) = &r {
                    lift_land = (*l, *n);
                }
                if let Some(d) = dev.as_mut() {
                    handle(d, r, &tx, &ctx, lift_land.0, lift_land.1);
                    let st = d.snapshot();
                    let _ = tx.send(Evt::State(Box::new(st)));
                    ctx.request_repaint();
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        // Synapse 能立即响应，是因为鼠标按下 DPI 键时会发送 hardwareEventReportID
        // （report 5）主动事件；我们只读 Feature Report，只能把轮询压到 250ms。
        // 阶梯表在 DPI 变化时或每 1s 读一次；单条命令失败不再直接判定休眠。
        let interval = if was_asleep {
            Duration::from_secs(3)
        } else if fails > 0 {
            Duration::from_secs(1)
        } else {
            Duration::from_millis(250)
        };
        if dev.is_some() && last_poll.elapsed() > interval {
            last_poll = Instant::now();
            let mut need_reconnect = false;
            if let Some(d) = dev.as_mut() {
                let dpi = d.dpi().ok();
                let mut profile = None;
                if dpi.is_some() {
                    let changed = dpi != last_dpi;
                    if changed || last_profile_poll.elapsed() > Duration::from_millis(1000) {
                        profile = d.dpi_profile().ok();
                        last_profile_poll = Instant::now();
                    }
                } else {
                    // DPI 读失败时再用阶梯表做一次探活，避免单条命令超时误判休眠。
                    profile = d.dpi_profile().ok();
                }
                let alive = dpi.is_some() || profile.is_some();
                let bat = if alive && last_battery.elapsed() > Duration::from_secs(5) {
                    last_battery = Instant::now();
                    d.battery().ok()
                } else {
                    None
                };
                if !alive {
                    fails += 1;
                    // 一次超时可能只是 Synapse 抢占共享缓冲区；连续 2 次才显示休眠。
                    if fails >= 2 && !was_asleep {
                        was_asleep = true;
                        let _ = tx.send(Evt::Sleeping(true));
                    }
                    // 连续多次失败才丢弃句柄，避免把偶发超时当成设备消失。
                    need_reconnect = fails >= 5;
                } else if was_asleep {
                    // 刚从休眠中醒来：重新探测能力（例如校准接口）
                    was_asleep = false;
                    fails = 0;
                    last_battery = Instant::now();
                    last_profile_poll = Instant::now();
                    last_dpi = dpi;
                    let _ = tx.send(Evt::Sleeping(false));
                    d.probe();
                    let _ = tx.send(Evt::State(Box::new(d.snapshot())));
                } else {
                    fails = 0;
                    let _ = tx.send(Evt::Sleeping(false));
                    let mut st = DeviceState::default();
                    if let Some(v) = dpi {
                        st.dpi = v;
                        last_dpi = Some(v);
                    }
                    if let Some(p) = profile {
                        st.profile = p;
                    }
                    if let Some(b) = bat {
                        st.battery = b;
                    }
                    st.name = d.product_name().to_string();
                    st.pid = d.info.product_id;
                    st.wireless = d.caps.wireless || d.spec.wireless;
                    st.max_dpi = d.spec.max_dpi;
                    st.max_poll = d.spec.max_poll;
                    st.has_rgb = d.spec.has_rgb;
                    st.caps = d.caps;
                    let _ = tx.send(Evt::Quick(Box::new(st)));
                }
                ctx.request_repaint();
            }
            if need_reconnect {
                dev = None;
                fails = 0;
                was_asleep = false;
                last_dpi = None;
                last_try = Instant::now() - Duration::from_secs(10);
                ctx.request_repaint();
            }
        }
    }
}

fn ok(tx: &Sender<Evt>, msg: &str) {
    let _ = tx.send(Evt::Toast(msg.to_string(), true));
}

fn fail(tx: &Sender<Evt>, msg: String) {
    let _ = tx.send(Evt::Toast(msg, false));
}

/// 把底层错误翻译成用户能懂的话
fn err_text(e: &rz_core::DeviceError) -> String {
    match e {
        rz_core::DeviceError::Timeout => "设备无应答（可能已休眠，动一下鼠标再试）".into(),
        rz_core::DeviceError::Unsupported => "设备不支持该设置".into(),
        other => other.to_string(),
    }
}

fn handle(d: &mut Device, req: Req, tx: &Sender<Evt>, _ctx: &egui::Context, state_lift: u8, state_land: u8) {
    match req {
        Req::Reconnect => {}
        Req::Refresh => {}
        Req::SetDpi(x, y) => match d.set_dpi(x, y) {
            Ok(_) => ok(tx, &format!("DPI 已设为 {} × {}", x, y)),
            Err(e) => fail(tx, format!("设置 DPI 失败: {}", err_text(&e))),
        },
        Req::SetPoll(hz) => match d.set_poll_hz(hz) {
            Ok(_) => {
                thread::sleep(Duration::from_millis(120));
                match d.poll_hz() {
                    Ok(v) if v == hz => ok(tx, &format!("轮询率已设为 {} Hz", hz)),
                    Ok(v) => fail(tx, format!("设备回报 {} Hz，未接受 {} Hz", v, hz)),
                    Err(e) => fail(tx, format!("轮询率校验失败: {}", e)),
                }
            }
            Err(e) => fail(tx, format!("设置轮询率失败: {}", err_text(&e))),
        },
        Req::SetSleepMin(min) => match d.set_sleep_secs(min * 60) {
            Ok(_) => ok(tx, &format!("休眠时间已设为 {} 分钟", min)),
            Err(e) => fail(tx, format!("设置休眠失败: {}", err_text(&e))),
        },
        Req::SetLowBatt(p) => match d.set_low_battery_percent(p) {
            Ok(_) => ok(tx, &format!("低电提示阈值已设为 {}%", p)),
            Err(e) => fail(tx, format!("设置阈值失败: {}", err_text(&e))),
        },
        Req::SetActiveStage(n) => match d.dpi_profile() {
            Ok(mut p) => {
                p.active = n;
                match d.set_dpi_profile(&p) {
                    Ok(_) => {
                        if let Some(s) = p.stages.get(n as usize - 1) {
                            let _ = d.set_dpi(s.x, s.y);
                        }
                        ok(tx, &format!("已切换到 DPI 第 {} 段", n));
                    }
                    Err(e) => fail(tx, format!("切换 DPI 段失败: {}", err_text(&e))),
                }
            }
            Err(e) => fail(tx, format!("读取 DPI 阶梯失败: {}", err_text(&e))),
        },
        Req::SetStageDpi(idx, dpi) => match d.dpi_profile() {
            Ok(mut p) => {
                if idx < p.stages.len() {
                    p.stages[idx] = DpiStage { x: dpi, y: dpi };
                    match d.set_dpi_profile(&p) {
                        Ok(_) => {
                            if p.active as usize == idx + 1 {
                                let _ = d.set_dpi(dpi, dpi);
                            }
                            ok(tx, &format!("第 {} 段 DPI 已设为 {}", idx + 1, dpi));
                        }
                        Err(e) => fail(tx, format!("写入 DPI 阶梯失败: {}", err_text(&e))),
                    }
                }
            }
            Err(e) => fail(tx, format!("读取 DPI 阶梯失败: {}", err_text(&e))),
        },
        Req::SetStages(values) => match d.dpi_profile() {
            Ok(mut p) => {
                for (i, v) in values.iter().enumerate() {
                    if i < p.stages.len() {
                        p.stages[i] = DpiStage { x: *v, y: *v };
                    }
                }
                match d.set_dpi_profile(&p) {
                    Ok(_) => {
                        if let Some(s) = p.stages.get(p.active as usize - 1) {
                            let _ = d.set_dpi(s.x, s.y);
                        }
                        ok(tx, "DPI 阶梯已写入");
                    }
                    Err(e) => fail(tx, format!("写入 DPI 阶梯失败: {}", err_text(&e))),
                }
            }
            Err(e) => fail(tx, format!("读取 DPI 阶梯失败: {}", err_text(&e))),
        },
        Req::SetBrightness(v) => match d.set_brightness(v) {
            Ok(_) => ok(tx, &format!("亮度已设为 {}%", v)),
            Err(e) => fail(tx, format!("设置亮度失败: {}", e)),
        },
        Req::SetColor(c) => match d.set_static_color(c[0], c[1], c[2]) {
            Ok(_) => ok(tx, "灯效颜色已下发"),
            Err(e) => fail(tx, format!("设置颜色失败: {}", e)),
        },
        Req::SetTracking(v) => match d.set_calib_param(1, v.clamp(0, 2)) {
            Ok(_) => ok(tx, &format!("追踪距离已设为 {}", ["低", "中等", "高"][v.min(2) as usize])),
            Err(e) => fail(tx, format!("设置追踪距离失败: {}", err_text(&e))),
        },
        Req::SetAsymmetric(on) => match d.set_calib_param(4, on as u8) {
            Ok(_) => {
                if on {
                    let _ = d.set_lift_land(state_lift, state_land);
                }
                ok(tx, if on { "已开启非对称中止" } else { "已关闭非对称中止" })
            }
            Err(e) => fail(tx, format!("设置非对称中止失败: {}", err_text(&e))),
        },
        Req::SetLiftLand(lift, land) => match d.set_lift_land(lift, land) {
            Ok(_) => ok(tx, &format!("抬升 {} / 着陆 {}", lift, land)),
            Err(e) => fail(tx, format!("写入抬升/着陆失败: {}", err_text(&e))),
        },
        Req::SetLiftoff(v) => match d.set_liftoff(v) {
            Ok(_) => ok(tx, &format!("追踪距离已设为 {}", ["低", "中等", "高"].get((v.max(1) - 1) as usize).copied().unwrap_or("中等"))),
            Err(e) => fail(tx, format!("设置追踪距离失败: {}", e)),
        },
        Req::CycleStage => match d.dpi_profile() {
            Ok(mut p) => {
                let n = p.stages.len().max(1) as u8;
                p.active = if p.active >= n { 1 } else { p.active + 1 };
                match d.set_dpi_profile(&p) {
                    Ok(_) => {
                        if let Some(s) = p.stages.get(p.active as usize - 1) {
                            let _ = d.set_dpi(s.x, s.y);
                            ok(tx, &format!("DPI 段 {} · {} DPI", p.active, s.x));
                        }
                    }
                    Err(e) => fail(tx, format!("切换 DPI 段失败: {}", err_text(&e))),
                }
            }
            Err(e) => fail(tx, format!("读取 DPI 阶梯失败: {}", err_text(&e))),
        },
    }
}

/// 供 UI 显示的调试信息（保留一个未使用命令的引用，便于后续扩展）
pub fn _unused(_c: Cmd) {}
