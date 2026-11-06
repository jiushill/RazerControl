//! 软件宏引擎：录制鼠标/键盘事件，回放时用 SendInput 重新注入。
//!
//! 为什么是软件宏：DeathAdder V3 Pro 的宏命令类（class 0x08）全部无应答、
//! 能力位图命令（0x00/0x00）也不支持，说明这代鼠标没有硬件宏存储。
//! Synapse 的宏对这类设备同样是纯软件实现（宏引擎在 PC 侧），所以照它做即可。

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Instant;

// ---------- 事件模型 ----------
pub const EV_MOVE: u8 = 0;
pub const EV_BTN_DOWN: u8 = 1;
pub const EV_BTN_UP: u8 = 2;
pub const EV_WHEEL: u8 = 3;
pub const EV_KEY_DOWN: u8 = 4;
pub const EV_KEY_UP: u8 = 5;

/// 鼠标按键编号（与 hook.rs 的槽位一致）：0 左 1 右 2 中 3 侧前 4 侧后
pub const BTN_LEFT: u8 = 0;
pub const BTN_RIGHT: u8 = 1;
pub const BTN_MIDDLE: u8 = 2;
pub const BTN_X1: u8 = 3;
pub const BTN_X2: u8 = 4;

#[derive(Clone, Copy, Debug, Default)]
pub struct Ev {
    /// 距上一条事件的间隔（毫秒）
    pub dt: u16,
    pub kind: u8,
    pub code: u8,
    pub val: i32,
}

impl Ev {
    pub fn new(dt: u16, kind: u8, code: u8, val: i32) -> Self {
        Ev { dt, kind, code, val }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Macro {
    pub name: String,
    pub events: Vec<Ev>,
}

impl Macro {
    pub fn duration_ms(&self) -> u32 {
        self.events.iter().map(|e| e.dt as u32).sum()
    }
    pub fn summary(&self) -> String {
        let n_move = self.events.iter().filter(|e| e.kind == EV_MOVE).count();
        let n_btn = self
            .events
            .iter()
            .filter(|e| e.kind == EV_BTN_DOWN || e.kind == EV_BTN_UP)
            .count();
        let n_key = self
            .events
            .iter()
            .filter(|e| e.kind == EV_KEY_DOWN || e.kind == EV_KEY_UP)
            .count();
        format!(
            "{} 事件 · {:.1}s · 移动 {} / 按键 {} / 键盘 {}",
            self.events.len(),
            self.duration_ms() as f32 / 1000.0,
            n_move,
            n_btn,
            n_key
        )
    }
}

// ---------- 录制 ----------
struct Rec {
    events: Vec<Ev>,
    last: Instant,
    /// 上一次鼠标绝对坐标（低级钩子给的是绝对位置，回放需要增量）
    last_pt: Option<(i32, i32)>,
}

static REC: Mutex<Option<Rec>> = Mutex::new(None);
static PLAYING: Mutex<bool> = Mutex::new(false);

pub fn is_recording() -> bool {
    REC.lock().map(|g| g.is_some()).unwrap_or(false)
}

pub fn is_playing() -> bool {
    PLAYING.lock().map(|g| *g).unwrap_or(false)
}

pub fn event_count() -> usize {
    REC.lock()
        .ok()
        .and_then(|g| g.as_ref().map(|r| r.events.len()))
        .unwrap_or(0)
}

pub fn start_recording() {
    if let Ok(mut g) = REC.lock() {
        *g = Some(Rec {
            events: Vec::new(),
            last: Instant::now(),
            last_pt: None,
        });
    }
}

pub fn stop_recording() -> Option<Vec<Ev>> {
    REC.lock().ok().and_then(|mut g| g.take()).map(|r| r.events)
}

/// 供钩子调用：记录一条事件（未在录制时直接返回）。
pub fn push(kind: u8, code: u8, val: i32) {
    if let Ok(mut g) = REC.lock() {
        if let Some(r) = g.as_mut() {
            let now = Instant::now();
            let dt = now.duration_since(r.last).as_millis().min(65_535) as u16;
            r.last = now;
            r.events.push(Ev::new(dt, kind, code, val));
        }
    }
}

/// 记录鼠标移动（传入绝对坐标，内部换算成增量）。
pub fn push_move(x: i32, y: i32) {
    if let Ok(mut g) = REC.lock() {
        if let Some(r) = g.as_mut() {
            let (dx, dy) = match r.last_pt {
                Some((px, py)) => (x - px, y - py),
                None => (0, 0),
            };
            r.last_pt = Some((x, y));
            if dx == 0 && dy == 0 {
                return;
            }
            let now = Instant::now();
            let dt = now.duration_since(r.last).as_millis().min(65_535) as u16;
            r.last = now;
            r.events.push(Ev::new(dt, EV_MOVE, 0, (dx << 16) | (dy & 0xFFFF)));
        }
    }
}

// ---------- 回放 ----------
#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MouseInput {
    dx: i32,
    dy: i32,
    mouse_data: u32,
    flags: u32,
    time: u32,
    extra: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct KeybdInput {
    vk: u16,
    scan: u16,
    flags: u32,
    time: u32,
    extra: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct HardwareInput {
    msg: u32,
    param_l: u16,
    param_h: u16,
}

#[repr(C)]
union InputUnion {
    mi: MouseInput,
    ki: KeybdInput,
    hi: HardwareInput,
}

#[repr(C)]
struct Input {
    kind: u32,
    u: InputUnion,
}

const MOUSEEVENTF_MOVE: u32 = 0x0001;
const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
const MOUSEEVENTF_MIDDLEDOWN: u32 = 0x0020;
const MOUSEEVENTF_MIDDLEUP: u32 = 0x0040;
const MOUSEEVENTF_XDOWN: u32 = 0x0080;
const MOUSEEVENTF_XUP: u32 = 0x0100;
const MOUSEEVENTF_WHEEL: u32 = 0x0800;
const KEYEVENTF_KEYUP: u32 = 0x0002;

#[link(name = "user32")]
extern "system" {
    fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
}

unsafe fn inject_mouse(flags: u32, data: u32, dx: i32, dy: i32) {
    let input = Input {
        kind: 0,
        u: InputUnion {
            mi: MouseInput {
                dx,
                dy,
                mouse_data: data,
                flags,
                time: 0,
                extra: 0,
            },
        },
    };
    SendInput(1, &input, std::mem::size_of::<Input>() as i32);
}

unsafe fn inject_key(vk: u16, up: bool) {
    let input = Input {
        kind: 1,
        u: InputUnion {
            ki: KeybdInput {
                vk,
                scan: 0,
                flags: if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                extra: 0,
            },
        },
    };
    SendInput(1, &input, std::mem::size_of::<Input>() as i32);
}

/// 回放一个宏（新线程，不阻塞 UI）。speed > 1 表示加速。
pub fn play(events: &[Ev], speed: f32) {
    if events.is_empty() {
        return;
    }
    if let Ok(mut p) = PLAYING.lock() {
        if *p {
            return; // 正在回放，忽略重复触发
        }
        *p = true;
    }
    let evs: Vec<Ev> = events.to_vec();
    let factor = if speed <= 0.0 { 1.0 } else { speed };
    std::thread::spawn(move || {
        for e in evs {
            let ms = (e.dt as f32 / factor) as u64;
            if ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
            unsafe {
                match e.kind {
                    EV_MOVE => {
                        let dx = (e.val >> 16) as i16 as i32;
                        let dy = e.val as i16 as i32;
                        inject_mouse(MOUSEEVENTF_MOVE, 0, dx, dy);
                    }
                    EV_BTN_DOWN => match e.code {
                        BTN_LEFT => inject_mouse(MOUSEEVENTF_LEFTDOWN, 0, 0, 0),
                        BTN_RIGHT => inject_mouse(MOUSEEVENTF_RIGHTDOWN, 0, 0, 0),
                        BTN_MIDDLE => inject_mouse(MOUSEEVENTF_MIDDLEDOWN, 0, 0, 0),
                        BTN_X1 => inject_mouse(MOUSEEVENTF_XDOWN, 1 << 16, 0, 0),
                        _ => inject_mouse(MOUSEEVENTF_XDOWN, 2 << 16, 0, 0),
                    },
                    EV_BTN_UP => match e.code {
                        BTN_LEFT => inject_mouse(MOUSEEVENTF_LEFTUP, 0, 0, 0),
                        BTN_RIGHT => inject_mouse(MOUSEEVENTF_RIGHTUP, 0, 0, 0),
                        BTN_MIDDLE => inject_mouse(MOUSEEVENTF_MIDDLEUP, 0, 0, 0),
                        BTN_X1 => inject_mouse(MOUSEEVENTF_XUP, 1 << 16, 0, 0),
                        _ => inject_mouse(MOUSEEVENTF_XUP, 2 << 16, 0, 0),
                    },
                    EV_WHEEL => inject_mouse(MOUSEEVENTF_WHEEL, e.val as u32, 0, 0),
                    EV_KEY_DOWN => inject_key(e.code as u16, false),
                    EV_KEY_UP => inject_key(e.code as u16, true),
                    _ => {}
                }
            }
        }
        if let Ok(mut p) = PLAYING.lock() {
            *p = false;
        }
    });
}


// ---------- 人类可读描述 ----------
pub fn btn_name(b: u8) -> &'static str {
    match b {
        BTN_LEFT => "鼠标左键",
        BTN_RIGHT => "鼠标右键",
        BTN_MIDDLE => "滚轮中键",
        BTN_X1 => "侧键 · 前进",
        _ => "侧键 · 后退",
    }
}

pub fn key_name(vk: u8) -> String {
    match vk {
        0x41..=0x5A => ((vk as u8) as char).to_string(),
        0x30..=0x39 => ((vk as u8) as char).to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0x20 => "空格".into(),
        0x0D => "回车".into(),
        0x1B => "Esc".into(),
        0x09 => "Tab".into(),
        0x08 => "退格".into(),
        0x10 => "Shift".into(),
        0x11 => "Ctrl".into(),
        0x12 => "Alt".into(),
        0x5B => "Win".into(),
        0x25 => "←".into(),
        0x26 => "↑".into(),
        0x27 => "→".into(),
        0x28 => "↓".into(),
        0x2C => "PrtSc".into(),
        0x2D => "Insert".into(),
        0x2E => "Delete".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        0x21 => "PgUp".into(),
        0x22 => "PgDn".into(),
        0xBA => ";".into(),
        0xBB => "=".into(),
        0xBC => ",".into(),
        0xBD => "-".into(),
        0xBE => ".".into(),
        0xBF => "/".into(),
        0xC0 => "`".into(),
        _ => format!("VK_{:02X}", vk),
    }
}

/// 一行事件描述，例如「鼠标左键 按下」「移动 dx=12 dy=-3」「A 按下」
pub fn ev_label(e: &Ev) -> String {
    match e.kind {
        EV_MOVE => {
            let dx = (e.val >> 16) as i16 as i32;
            let dy = e.val as i16 as i32;
            format!("移动  dx={} dy={}", dx, dy)
        }
        EV_BTN_DOWN => format!("{} 按下", btn_name(e.code)),
        EV_BTN_UP => format!("{} 抬起", btn_name(e.code)),
        EV_WHEEL => format!("滚轮 {}", if e.val > 0 { "上" } else { "下" }),
        EV_KEY_DOWN => format!("{} 按下", key_name(e.code)),
        EV_KEY_UP => format!("{} 抬起", key_name(e.code)),
        _ => "未知事件".to_string(),
    }
}

/// 事件大类（用于着色/分组）
pub fn ev_kind_name(e: &Ev) -> &'static str {
    match e.kind {
        EV_MOVE => "移动",
        EV_BTN_DOWN | EV_BTN_UP => "鼠标",
        EV_WHEEL => "滚轮",
        EV_KEY_DOWN | EV_KEY_UP => "键盘",
        _ => "其它",
    }
}

// ---------- 存储 ----------
fn dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("RazerControl")
}

fn path() -> PathBuf {
    dir().join("macros.txt")
}

/// 文本格式：
/// ```text
/// [宏名]
/// 12,1,0,8
/// 40,2,0,8
/// ```
pub fn load_all() -> Vec<Macro> {
    let p = path();
    let text = match fs::read_to_string(&p) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<Macro> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            out.push(Macro {
                name: line[1..line.len() - 1].to_string(),
                events: Vec::new(),
            });
            continue;
        }
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() != 4 || out.is_empty() {
            continue;
        }
        let (dt, kind, code, val) = (
            parts[0].parse::<u16>().ok(),
            parts[1].parse::<u8>().ok(),
            parts[2].parse::<u8>().ok(),
            parts[3].parse::<i32>().ok(),
        );
        if let (Some(dt), Some(kind), Some(code), Some(val)) = (dt, kind, code, val) {
            if let Some(m) = out.last_mut() {
                m.events.push(Ev::new(dt, kind, code, val));
            }
        }
    }
    out
}

pub fn save_all(list: &[Macro]) -> std::io::Result<()> {
    let _ = fs::create_dir_all(dir());
    let mut s = String::new();
    s.push_str("# RazerControl 宏（文本格式，可直接编辑）\n");
    for m in list {
        s.push_str(&format!("[{}]\n", m.name.replace(['\n', '\r', ']'], "")));
        for e in &m.events {
            s.push_str(&format!("{},{},{},{}\n", e.dt, e.kind, e.code, e.val));
        }
    }
    fs::write(path(), s)
}

pub fn store_path() -> String {
    path().to_string_lossy().to_string()
}

/// 是否已经用过（用于 UI 只在首次提示路径）
pub static NOTICED: OnceLock<bool> = OnceLock::new();
