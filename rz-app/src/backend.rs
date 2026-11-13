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

/// 后端诊断日志：写 %APPDATA%\RazerControl\backend.log（超过 256KB 自动截断重来）。
/// 间歇性问题（状态变"未知"、校准突然不可用、误判休眠）靠它抓现场。
pub fn diag(msg: &str) {
    use std::io::Write;
    let Some(dir) = std::env::var("APPDATA").ok() else {
        return;
    };
    let path = format!("{dir}\\RazerControl\\backend.log");
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > 256 * 1024 {
            let _ = std::fs::remove_file(&path);
        }
    }
    static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let ms = T0.get_or_init(std::time::Instant::now).elapsed().as_millis();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "[{ms:>8}ms] {msg}");
    }
}

/// UI 是否在前台/可见（决定轮询频率）。前台 150ms，可见 500ms，托盘 2s。
static UI_FOCUSED: AtomicBool = AtomicBool::new(true);
static UI_VISIBLE: AtomicBool = AtomicBool::new(true);

/// 由 UI 线程调用：窗口焦点/可见性变化
pub fn set_ui_state(focused: bool, visible: bool) {
    UI_FOCUSED.store(focused, Ordering::Relaxed);
    UI_VISIBLE.store(visible, Ordering::Relaxed);
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
    let mut wakes: u32 = 0;
    let mut last_ok = Instant::now();
    let mut lift_land: (u8, u8) = (15, 8);

    loop {
        if dev.is_none() && last_try.elapsed() > Duration::from_millis(1200) {
            last_try = Instant::now();
            match pick_device() {
                Some(mut d) => {
                    let st = d.snapshot();
                    diag(&format!(
                        "connect pid={:04X} name={} wireless={} caps={{dpi:{} poll:{} bat:{} calib:{}}}",
                        d.info.product_id,
                        d.product_name(),
                        d.caps.wireless || d.spec.wireless,
                        st.caps.dpi,
                        st.caps.poll_rate,
                        st.caps.battery,
                        st.caps.calibration
                    ));
                    if !st.caps.dpi || !st.caps.battery {
                        diag(&format!(
                            "connect 快照残缺: dpi={:?} poll={} bat={:?} stages={}",
                            Some(st.dpi),
                            st.poll_hz,
                            st.battery,
                            st.profile.stages.len()
                        ));
                    }
                    connected.store(true, Ordering::Relaxed);
                    lost_reported = false;
                    let responsive = st.caps.dpi || st.caps.poll_rate || st.caps.battery;
                    if responsive {
                        last_ok = Instant::now();
                        let _ = tx.send(Evt::Sleeping(false));
                        let _ = tx.send(Evt::State(Box::new(st)));
                    } else {
                        // 接口在、但一条命令都不应答：设备在低功耗态，只能靠物理移动唤醒
                        diag("connect 但设备不应答（caps 全假）→ 判定休眠，等物理唤醒");
                        was_asleep = true;
                        wakes = 0;
                        fails = 0;
                        last_ok = Instant::now();
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
                    let tag = format!("{:?}", r).chars().take(40).collect::<String>();
                    handle(d, r, &tx, &ctx, lift_land.0, lift_land.1);
                    let st = d.snapshot();
                    if !st.caps.dpi || !st.caps.battery {
                        diag(&format!("请求后快照残缺: req={tag} dpi={:?} bat={:?}", st.dpi, st.battery));
                    }
                    let _ = tx.send(Evt::State(Box::new(st)));
                    ctx.request_repaint();
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        // 无线鼠标在静止间隙偶尔不回包是正常的，不能当成休眠。
        // 策略：每轮只发一条命令；连续多次失败才判定休眠，恢复要连续两次成功（迟滞）。
        let interval = if was_asleep {
            Duration::from_secs(3)
        } else if fails > 0 {
            Duration::from_secs(2)
        } else if UI_FOCUSED.load(Ordering::Relaxed) {
            // 用户正盯着界面：150ms，切 DPI 段后几乎立刻刷新
            Duration::from_millis(150)
        } else if UI_VISIBLE.load(Ordering::Relaxed) {
            // 窗口可见但没聚焦：降到 3s。Synapse 在这种状态下根本不发帧，
            // 我们保留低频探测只是为了电量和 DPI 不会太旧。
            Duration::from_secs(3)
        } else {
            // 收进托盘：30s 一次孤探测。设备空闲约 63s 会进低功耗，
            // 频繁去问只会不断撞上 0x04，没有任何收益。
            Duration::from_secs(30)
        };
        if dev.is_some() && last_poll.elapsed() > interval {
            last_poll = Instant::now();
            let mut need_reconnect = false;
            if let Some(d) = dev.as_mut() {
                // 只发一条：DPI 读取本身就是探活，失败时不要紧接着再发第二条
                let dpi = d.dpi().ok();
                let mut profile = None;
                if let Some(v) = dpi {
                    let changed = dpi != last_dpi;
                    if changed && last_profile_poll.elapsed() > Duration::from_millis(800) {
                        profile = d.dpi_profile().ok();
                        last_profile_poll = Instant::now();
                    }
                    let _ = v;
                }
                let alive = dpi.is_some();
                let bat = if alive && last_battery.elapsed() > Duration::from_secs(30) {
                    last_battery = Instant::now();
                    d.battery().ok()
                } else {
                    None
                };
                if !alive {
                    fails += 1;
                    if fails == 1 || fails == 5 {
                        diag(&format!(
                            "poll 失败 #{fails}（连续），已静默 {}s",
                            last_ok.elapsed().as_secs()
                        ));
                    }
                    // 设备空闲约 63s 就会进入低功耗并回 0x04 —— 这不是掉线。
                    // 只有「连续 30 秒没有任何一次成功响应」才认为真休眠，
                    // 期间界面保持最后一次读数（与 Synapse 的观感一致）。
                    if fails >= 5 && last_ok.elapsed() > Duration::from_secs(30) && !was_asleep {
                        diag(&format!(
                            "判定休眠（连续失败 {} 次，无成功响应 {}s）",
                            fails,
                            last_ok.elapsed().as_secs()
                        ));
                        was_asleep = true;
                        wakes = 0;
                        let _ = tx.send(Evt::Sleeping(true));
                    }
                    // 更久才丢弃句柄，避免把抖动当成设备消失
                    need_reconnect = fails >= 20;
                    if need_reconnect {
                        diag(&format!("丢弃句柄重连（连续失败 {} 次）", fails));
                    }
                } else if was_asleep {
                    last_ok = Instant::now();
                    // 迟滞：连续两次成功才认为真的醒了，避免来回跳
                    wakes += 1;
                    if wakes < 2 {
                        ctx.request_repaint();
                        continue;
                    }
                    // 刚从休眠中醒来：重新探测能力（例如校准接口）
                    diag(&format!("从休眠恢复（静默 {}s）", last_ok.elapsed().as_secs()));
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
                    wakes = 0;
                    last_ok = Instant::now();
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
