//! 软件宏引擎 —— 模型照搬 Razer Synapse（逆向自其 IndexedDB 存储）。
//!
//! 逆向依据（`%LOCALAPPDATA%\Razer\RazerAppEngine\User Data\Default\IndexedDB` 里
//! `RZR_<deviceId>/macro/<id>` 的值）：
//!
//! ```json
//! { "name":"Macro 1", "appEngine": {
//!     "events":[ {"type":"delay","ms":966},
//!                {"type":"keyboard","scancode":30,"flag":1},
//!                {"type":"delay","ms":129},
//!                {"type":"keyboard","scancode":30,"flag":2}, ... ],
//!     "mouseMoveType":"none", "macroType":0, "delaySetting":0,
//!     "recordProfile":{ "startDelay":0, "shortcutKey":0, "delayTime":0.2,
//!                       "mmtSetting":0, "delaySetting":0,
//!                       "randomizeDelay":{"min":0.2,"max":0.5}, "macroType":0 } } }
//! ```
//!
//! 三个关键点（与"自己设计"的差别）：
//! 1. **延迟是独立事件**，和按键交替排在同一个数组里；
//! 2. 键盘用 **scan code + flag**（1=按下 2=抬起），不用虚拟键码；
//! 3. 鼠标移动追踪是字符串枚举，且记录的是**绝对坐标**，回放时按模式换算。

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

// ---------- 事件（对应 Synapse 的 events 数组） ----------
pub const T_DELAY: u8 = 0;
pub const T_KEYBOARD: u8 = 1;
pub const T_MOUSE: u8 = 2;
pub const T_MOUSE_MOVE: u8 = 3;
pub const T_WHEEL: u8 = 4;

/// flag: 1 = 按下，2 = 抬起（与雷云一致）
pub const FLAG_DOWN: u8 = 1;
pub const FLAG_UP: u8 = 2;

/// 鼠标键：0 左 1 右 2 中 3 侧键·前进 4 侧键·后退
pub const BTN_LEFT: u8 = 0;
pub const BTN_RIGHT: u8 = 1;
pub const BTN_MIDDLE: u8 = 2;
pub const BTN_X1: u8 = 3;
pub const BTN_X2: u8 = 4;

/// 扩展键在 scancode 高位置 0xE000（回放时转成 KEYEVENTF_EXTENDEDKEY）
pub const SC_EXT: u16 = 0xE000;

#[derive(Clone, Copy, Debug, Default)]
pub struct Ev {
    pub kind: u8,
    /// delay: 毫秒；keyboard: scancode；mouse: 键号；wheel: 增量
    pub a: i32,
    /// keyboard/mouse: flag；mouse_move: y
    pub b: i32,
    /// mouse_move: x
    pub c: i32,
}

impl Ev {
    pub fn delay(ms: u32) -> Self {
        Ev {
            kind: T_DELAY,
            a: ms as i32,
            ..Default::default()
        }
    }
    pub fn key(sc: u16, down: bool) -> Self {
        Ev {
            kind: T_KEYBOARD,
            a: sc as i32,
            b: if down { FLAG_DOWN as i32 } else { FLAG_UP as i32 },
            c: 0,
        }
    }
    pub fn mouse(btn: u8, down: bool) -> Self {
        Ev {
            kind: T_MOUSE,
            a: btn as i32,
            b: if down { FLAG_DOWN as i32 } else { FLAG_UP as i32 },
            c: 0,
        }
    }
    pub fn wheel(delta: i32) -> Self {
        Ev {
            kind: T_WHEEL,
            a: delta,
            ..Default::default()
        }
    }
    pub fn move_abs(x: i32, y: i32) -> Self {
        Ev {
            kind: T_MOUSE_MOVE,
            a: 0,
            b: y,
            c: x,
        }
    }
}

// ---------- 参数（对应 recordProfile） ----------
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DelayMode {
    /// 0 录制延迟
    Recorded,
    /// 1 固定延迟（秒）
    Fixed,
    /// 2 随机延迟（秒，min..max）
    Random,
    /// 3 无延迟
    None,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MoveMode {
    /// 无
    None,
    /// 屏幕（绝对位置）
    Absolute,
    /// 前台（相对位置）
    Relative,
    /// 起始点（鼠标光标）
    Origin,
}

#[derive(Clone, Copy, Debug)]
pub struct Profile {
    /// 启动延迟（毫秒）
    pub start_delay_ms: u32,
    /// 快捷键（虚拟键码，0 = 未设置）
    pub shortcut: u8,
    /// 固定延迟（秒）
    pub delay_time: f32,
    /// 随机延迟区间（秒）
    pub rand_min: f32,
    pub rand_max: f32,
    pub delay_mode: DelayMode,
    pub move_mode: MoveMode,
    /// 宏类型：0 标准 / 1 序列 / 2 分类（当前仅标准可执行）
    pub macro_type: u8,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            start_delay_ms: 0,
            shortcut: 0,
            delay_time: 0.2,
            rand_min: 0.2,
            rand_max: 0.5,
            delay_mode: DelayMode::Recorded,
            move_mode: MoveMode::Relative,
            macro_type: 0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Macro {
    pub name: String,
    pub events: Vec<Ev>,
    pub profile: Profile,
    /// 录制起点（鼠标位置），用于「起始点」模式
    pub origin: (i32, i32),
}

impl Macro {
    pub fn duration_ms(&self) -> u32 {
        self.events
            .iter()
            .filter(|e| e.kind == T_DELAY)
            .map(|e| e.a.max(0) as u32)
            .sum()
    }
    pub fn summary(&self) -> String {
        let n_key = self.events.iter().filter(|e| e.kind == T_KEYBOARD).count();
        let n_btn = self.events.iter().filter(|e| e.kind == T_MOUSE).count();
        let n_move = self.events.iter().filter(|e| e.kind == T_MOUSE_MOVE).count();
        let n_delay = self.events.iter().filter(|e| e.kind == T_DELAY).count();
        let mut extra = Vec::new();
        if self.profile.shortcut != 0 {
            extra.push(format!("快捷键 {}", key_name(self.profile.shortcut)));
        }
        match self.profile.delay_mode {
            DelayMode::Fixed => extra.push(format!("固定 {:.2}s", self.profile.delay_time)),
            DelayMode::Random => extra.push(format!(
                "随机 {:.2}-{:.2}s",
                self.profile.rand_min, self.profile.rand_max
            )),
            DelayMode::None => extra.push("无延迟".to_string()),
            DelayMode::Recorded => {}
        }
        if self.profile.move_mode == MoveMode::None {
            extra.push("不录移动".to_string());
        }
        let tail = if extra.is_empty() {
            String::new()
        } else {
            format!(" · {}", extra.join(" · "))
        };
        format!(
            "{} 事件（延迟 {}）· {:.2}s · 按键 {} / 鼠标 {} / 移动 {}{}",
            self.events.len(),
            n_delay,
            self.duration_ms() as f32 / 1000.0,
            n_key / 2,
            n_btn / 2,
            n_move,
            tail
        )
    }
}

// ---------- 录制 ----------
struct Rec {
    events: Vec<Ev>,
    last: Instant,
    origin: (i32, i32),
    has_origin: bool,
    track_moves: bool,
}

static REC: Mutex<Option<Rec>> = Mutex::new(None);
static PLAYING: Mutex<bool> = Mutex::new(false);

/// 取录制锁：即使之前有线程 panic 污染了锁也照常使用，
/// 否则录制会静默失效（界面显示"录制中"但一步都记不到）。
fn rec_lock() -> std::sync::MutexGuard<'static, Option<Rec>> {
    REC.lock().unwrap_or_else(|e| e.into_inner())
}

fn playing_lock() -> std::sync::MutexGuard<'static, bool> {
    PLAYING.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn is_recording() -> bool {
    rec_lock().is_some()
}
pub fn is_playing() -> bool {
    *playing_lock()
}
/// 录制中的事件快照（UI 实时显示用）
pub fn recording_snapshot() -> Vec<Ev> {
    rec_lock()
        .as_ref()
        .map(|r| r.events.clone())
        .unwrap_or_default()
}

pub fn event_count() -> usize {
    rec_lock().as_ref().map(|r| r.events.len()).unwrap_or(0)
}

pub fn start_recording(track_moves: bool) {
    {
        let mut g = rec_lock();
        *g = Some(Rec {
            events: Vec::new(),
            last: Instant::now(),
            origin: (0, 0),
            has_origin: false,
            track_moves,
        });
    }
}

pub fn stop_recording() -> Option<(Vec<Ev>, (i32, i32))> {
    rec_lock().take().map(|mut r| {
        strip_stop_clicks(&mut r.events);
        (r.events, r.origin)
    })
}

/// 点「停止」这个动作本身是靠左键完成的，会被钩子录进去。
/// 雷云的做法是把它剔掉 —— 我们也剔掉尾部的左键按下/抬起。
fn strip_stop_clicks(events: &mut Vec<Ev>) {
    for _ in 0..2 {
        // 找到尾部最后一个非延迟事件
        let mut i = events.len();
        while i > 0 && events[i - 1].kind == T_DELAY {
            i -= 1;
        }
        if i == 0 {
            return;
        }
        let last = i - 1;
        let is_left_up = events[last].kind == T_MOUSE
            && events[last].a == BTN_LEFT as i32
            && events[last].b as u8 == FLAG_UP;
        if !is_left_up {
            return;
        }
        // 往前找配对的按下
        let mut j = last;
        let mut down_at = None;
        while j > 0 {
            j -= 1;
            if events[j].kind == T_DELAY {
                continue;
            }
            if events[j].kind == T_MOUSE
                && events[j].a == BTN_LEFT as i32
                && events[j].b as u8 == FLAG_DOWN
            {
                down_at = Some(j);
            }
            break;
        }
        match down_at {
            Some(k) => {
                events.truncate(k);
                while events.last().map(|e| e.kind == T_DELAY).unwrap_or(false) {
                    events.pop();
                }
            }
            None => return,
        }
    }
}

/// 追加一条事件；自动在它前面插入「距上一条的延迟」事件（与雷云同构）
fn append(ev: Ev) {
    {
        let mut g = rec_lock();
        if let Some(r) = g.as_mut() {
            let now = Instant::now();
            let dt = now.duration_since(r.last).as_millis().min(60_000) as u32;
            r.last = now;
            r.events.push(Ev::delay(dt));
            r.events.push(ev);
        }
    }
}

/// 该步骤是否被用户勾掉（不参与回放）
pub fn ev_disabled(e: &Ev) -> bool {
    (e.c & 1) != 0
}

pub fn ev_set_disabled(e: &mut Ev, off: bool) {
    if off {
        e.c |= 1;
    } else {
        e.c &= !1;
    }
}

/// 合计时长（毫秒），只算启用的延迟步骤
pub fn total_ms(events: &[Ev]) -> u32 {
    events
        .iter()
        .filter(|e| e.kind == T_DELAY && !ev_disabled(e))
        .map(|e| e.a.max(0) as u32)
        .sum()
}

pub fn push_key(scancode: u16, down: bool) {
    append(Ev::key(scancode, down));
}
pub fn push_mouse(btn: u8, down: bool) {
    append(Ev::mouse(btn, down));
}
pub fn push_wheel(delta: i32) {
    append(Ev::wheel(delta));
}
/// 鼠标移动（绝对坐标；回放时按模式换算）
pub fn push_move(x: i32, y: i32) {
    if let Ok(mut g) = REC.lock() {
        if let Some(r) = g.as_mut() {
            if !r.has_origin {
                r.origin = (x, y);
                r.has_origin = true;
            }
            if !r.track_moves {
                return;
            }
        } else {
            return;
        }
    }
    append(Ev::move_abs(x, y));
}

// ---------- 回放 ----------
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
const KEYEVENTF_EXTENDEDKEY: u32 = 0x0001;
const KEYEVENTF_KEYUP: u32 = 0x0002;
const KEYEVENTF_SCANCODE: u32 = 0x0008;

#[link(name = "user32")]
extern "system" {
    fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
    fn SetCursorPos(x: i32, y: i32) -> i32;
    fn GetCursorPos(p: *mut (i32, i32)) -> i32;
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

unsafe fn inject_scancode(sc: u16, down: bool) {
    let ext = sc & SC_EXT != 0;
    let code = sc & 0x00FF;
    let mut flags = KEYEVENTF_SCANCODE;
    if ext {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    let input = Input {
        kind: 1,
        u: InputUnion {
            ki: KeybdInput {
                vk: 0,
                scan: code,
                flags,
                time: 0,
                extra: 0,
            },
        },
    };
    SendInput(1, &input, std::mem::size_of::<Input>() as i32);
}

fn rand_between(a: f32, b: f32) -> u32 {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    if hi <= lo {
        return (lo * 1000.0) as u32;
    }
    let r = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as f32 / 1e9)
        .unwrap_or(0.5);
    ((lo + (hi - lo) * r) * 1000.0) as u32
}

/// 回放（新线程）。speed > 1 加速，delay_mode 覆盖录制延迟。
pub fn play(m: &Macro, speed: f32) {
    if m.events.is_empty() {
        return;
    }
    if let Ok(mut p) = PLAYING.lock() {
        if *p {
            return;
        }
        *p = true;
    }
    let evs = m.events.clone();
    let p = m.profile;
    let factor = if speed <= 0.0 { 1.0 } else { speed };
    let origin = m.origin;

    std::thread::spawn(move || {
        if p.start_delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(
                (p.start_delay_ms as f32 / factor) as u64,
            ));
        }
        // 「起始点」：先把光标挪回录制起点
        let mut prev: Option<(i32, i32)> = None;
        if p.move_mode == MoveMode::Origin && (origin.0 != 0 || origin.1 != 0) {
            unsafe {
                SetCursorPos(origin.0, origin.1);
            }
            prev = Some(origin);
        } else if p.move_mode == MoveMode::Relative {
            let mut cur = (0i32, 0i32);
            unsafe {
                GetCursorPos(&mut cur as *mut (i32, i32));
            }
            prev = Some(cur);
        }

        for e in evs {
            if ev_disabled(&e) {
                continue;
            }
            unsafe {
                match e.kind {
                    T_DELAY => {
                        let ms = match p.delay_mode {
                            DelayMode::Fixed => (p.delay_time * 1000.0 / factor) as u64,
                            DelayMode::Random => (rand_between(p.rand_min, p.rand_max) as f32 / factor) as u64,
                            DelayMode::None => 0,
                            DelayMode::Recorded => (e.a.max(0) as f32 / factor) as u64,
                        };
                        if ms > 0 {
                            std::thread::sleep(std::time::Duration::from_millis(ms));
                        }
                    }
                    T_KEYBOARD => {
                        inject_scancode(e.a as u16, e.b == FLAG_DOWN as i32);
                    }
                    T_MOUSE => {
                        let down = e.b == FLAG_DOWN as i32;
                        match e.a as u8 {
                            BTN_LEFT => inject_mouse(
                                if down { MOUSEEVENTF_LEFTDOWN } else { MOUSEEVENTF_LEFTUP },
                                0, 0, 0,
                            ),
                            BTN_RIGHT => inject_mouse(
                                if down { MOUSEEVENTF_RIGHTDOWN } else { MOUSEEVENTF_RIGHTUP },
                                0, 0, 0,
                            ),
                            BTN_MIDDLE => inject_mouse(
                                if down { MOUSEEVENTF_MIDDLEDOWN } else { MOUSEEVENTF_MIDDLEUP },
                                0, 0, 0,
                            ),
                            BTN_X1 => inject_mouse(
                                if down { MOUSEEVENTF_XDOWN } else { MOUSEEVENTF_XUP },
                                1 << 16, 0, 0,
                            ),
                            _ => inject_mouse(
                                if down { MOUSEEVENTF_XDOWN } else { MOUSEEVENTF_XUP },
                                2 << 16, 0, 0,
                            ),
                        }
                    }
                    T_WHEEL => inject_mouse(MOUSEEVENTF_WHEEL, e.a as u32, 0, 0),
                    T_MOUSE_MOVE => match p.move_mode {
                        MoveMode::None => {}
                        MoveMode::Absolute => {
                            SetCursorPos(e.c, e.b);
                            prev = Some((e.c, e.b));
                        }
                        MoveMode::Relative | MoveMode::Origin => {
                            let (px, py) = prev.unwrap_or((0, 0));
                            let dx = e.c - px;
                            let dy = e.b - py;
                            if dx != 0 || dy != 0 {
                                inject_mouse(MOUSEEVENTF_MOVE, 0, dx, dy);
                            }
                            prev = Some((e.c, e.b));
                        }
                    },
                    _ => {}
                }
            }
        }
        if let Ok(mut p) = PLAYING.lock() {
            *p = false;
        }
    });
}

// ---------- 名称表 ----------
pub fn btn_name(b: u8) -> &'static str {
    match b {
        BTN_LEFT => "鼠标左键",
        BTN_RIGHT => "鼠标右键",
        BTN_MIDDLE => "滚轮中键",
        BTN_X1 => "侧键 · 前进",
        _ => "侧键 · 后退",
    }
}

/// 虚拟键码名（用于显示快捷键）
pub fn key_name(vk: u8) -> String {
    match vk {
        0x41..=0x5A => ((vk as u8) as char).to_string(),
        0x30..=0x39 => ((vk as u8) as char).to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0x20 => "空格".into(),
        0x0D => "回车".into(),
        0x1B => "Esc".into(),
        0x09 => "Tab".into(),
        0x10 => "Shift".into(),
        0x11 => "Ctrl".into(),
        0x12 => "Alt".into(),
        0x25 => "←".into(),
        0x26 => "↑".into(),
        0x27 => "→".into(),
        0x28 => "↓".into(),
        _ => format!("VK_{:02X}", vk),
    }
}

/// scancode 名（Set 1，去掉 0xE000 扩展位）
/// VK -> scan code（添加「键盘功能」步骤用）
pub fn vk_to_scancode(vk: u8) -> u16 {
    extern "system" {
        fn MapVirtualKeyW(code: u32, map_type: u32) -> u32;
    }
    let sc = unsafe { MapVirtualKeyW(vk as u32, 0) } as u16;
    if sc == 0 {
        vk as u16
    } else {
        sc
    }
}

pub fn sc_name(sc: u16) -> String {
    let ext = sc & SC_EXT != 0;
    let c = (sc & 0xFF) as u8;
    let base: &str = match c {
        0x01 => "Esc", 0x02 => "1", 0x03 => "2", 0x04 => "3", 0x05 => "4", 0x06 => "5",
        0x07 => "6", 0x08 => "7", 0x09 => "8", 0x0A => "9", 0x0B => "0",
        0x0C => "-", 0x0D => "=", 0x0E => "退格", 0x0F => "Tab",
        0x10 => "Q", 0x11 => "W", 0x12 => "E", 0x13 => "R", 0x14 => "T", 0x15 => "Y",
        0x16 => "U", 0x17 => "I", 0x18 => "O", 0x19 => "P", 0x1A => "[", 0x1B => "]",
        0x1C => "回车", 0x1D => "Ctrl", 0x1E => "A", 0x1F => "S", 0x20 => "D", 0x21 => "F",
        0x22 => "G", 0x23 => "H", 0x24 => "J", 0x25 => "K", 0x26 => "L", 0x27 => ";",
        0x28 => "'", 0x29 => "`", 0x2A => "Shift", 0x2B => "\\", 0x2C => "Z", 0x2D => "X",
        0x2E => "C", 0x2F => "V", 0x30 => "B", 0x31 => "N", 0x32 => "M", 0x33 => ",",
        0x34 => ".", 0x35 => "/", 0x36 => "右Shift", 0x37 => "小键盘*", 0x38 => "Alt",
        0x39 => "空格", 0x3A => "CapsLock",
        0x3B => "F1", 0x3C => "F2", 0x3D => "F3", 0x3E => "F4", 0x3F => "F5", 0x40 => "F6",
        0x41 => "F7", 0x42 => "F8", 0x43 => "F9", 0x44 => "F10", 0x57 => "F11", 0x58 => "F12",
        0x45 => "NumLock", 0x46 => "ScrollLock",
        0x47 => "小键盘7", 0x48 => "小键盘8", 0x49 => "小键盘9", 0x4A => "小键盘-",
        0x4B => "小键盘4", 0x4C => "小键盘5", 0x4D => "小键盘6", 0x4E => "小键盘+",
        0x4F => "小键盘1", 0x50 => "小键盘2", 0x51 => "小键盘3", 0x52 => "小键盘0",
        0x53 => "小键盘.",
        _ => "",
    };
    if ext {
        let e = match c {
            0x1C => "小键盘回车",
            0x1D => "右Ctrl",
            0x35 => "小键盘/",
            0x38 => "右Alt",
            0x47 => "Home",
            0x48 => "↑",
            0x49 => "PgUp",
            0x4B => "←",
            0x4D => "→",
            0x4F => "End",
            0x50 => "↓",
            0x51 => "PgDn",
            0x52 => "Insert",
            0x53 => "Delete",
            _ => "",
        };
        if !e.is_empty() {
            return e.to_string();
        }
    }
    if base.is_empty() {
        format!("SC_{:02X}", c)
    } else {
        base.to_string()
    }
}

pub fn ev_label(e: &Ev) -> String {
    match e.kind {
        T_DELAY => format!("延迟 {:.3}s", e.a.max(0) as f32 / 1000.0),
        T_KEYBOARD => format!(
            "{} {}",
            sc_name(e.a as u16),
            if e.b == FLAG_DOWN as i32 { "按下" } else { "抬起" }
        ),
        T_MOUSE => format!(
            "{} {}",
            btn_name(e.a as u8),
            if e.b == FLAG_DOWN as i32 { "按下" } else { "抬起" }
        ),
        T_WHEEL => format!("滚轮 {}", if e.a > 0 { "上" } else { "下" }),
        T_MOUSE_MOVE => format!("移动 ({}, {})", e.c, e.b),
        _ => "未知".to_string(),
    }
}

// ---------- 存储（文本，字段名与雷云 recordProfile 对齐） ----------
fn dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("RazerControl")
}
fn path() -> PathBuf {
    dir().join("macros.txt")
}

fn move_mode_str(m: MoveMode) -> &'static str {
    match m {
        MoveMode::None => "none",
        MoveMode::Absolute => "absolute",
        MoveMode::Relative => "relative",
        MoveMode::Origin => "origin",
    }
}
fn parse_move_mode(s: &str) -> MoveMode {
    match s {
        "none" => MoveMode::None,
        "absolute" => MoveMode::Absolute,
        "origin" => MoveMode::Origin,
        _ => MoveMode::Relative,
    }
}

fn parse_header(line: &str) -> Macro {
    let inner = &line[1..line.len() - 1];
    let mut parts = inner.split('|');
    let name = parts.next().unwrap_or("宏").trim().to_string();
    let mut m = Macro {
        name,
        ..Default::default()
    };
    for kv in parts {
        let mut it = kv.splitn(2, '=');
        let k = it.next().unwrap_or("").trim();
        let v = it.next().unwrap_or("").trim();
        match k {
            "start" => m.profile.start_delay_ms = v.parse().unwrap_or(0),
            "hot" => m.profile.shortcut = v.parse().unwrap_or(0),
            "delayTime" => m.profile.delay_time = v.parse().unwrap_or(0.2),
            "randMin" => m.profile.rand_min = v.parse().unwrap_or(0.2),
            "randMax" => m.profile.rand_max = v.parse().unwrap_or(0.5),
            "delaySetting" => {
                m.profile.delay_mode = match v {
                    "fixed" => DelayMode::Fixed,
                    "random" => DelayMode::Random,
                    "none" => DelayMode::None,
                    _ => DelayMode::Recorded,
                }
            }
            "mmt" => m.profile.move_mode = parse_move_mode(v),
            "macroType" => m.profile.macro_type = v.parse().unwrap_or(0),
            "ox" => m.origin.0 = v.parse().unwrap_or(0),
            "oy" => m.origin.1 = v.parse().unwrap_or(0),
            _ => {}
        }
    }
    m
}

pub fn load_all() -> Vec<Macro> {
    let text = match fs::read_to_string(path()) {
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
            out.push(parse_header(line));
            continue;
        }
        let p: Vec<&str> = line.split(',').collect();
        if p.len() != 4 || out.is_empty() {
            continue;
        }
        let kind = p[0].parse::<u8>().ok();
        let a = p[1].parse::<i32>().ok();
        let b = p[2].parse::<i32>().ok();
        let c = p[3].parse::<i32>().ok();
        if let (Some(kind), Some(a), Some(b), Some(c)) = (kind, a, b, c) {
            if let Some(m) = out.last_mut() {
                m.events.push(Ev { kind, a, b, c });
            }
        }
    }
    out
}

pub fn save_all(list: &[Macro]) -> std::io::Result<()> {
    let _ = fs::create_dir_all(dir());
    let mut s = String::new();
    s.push_str("# RazerControl 宏（模型对齐 Razer Synapse 的 recordProfile + events）\n");
    for m in list {
        s.push_str(&format!(
            "[{}|start={}|hot={}|delayTime={}|randMin={}|randMax={}|delaySetting={}|mmt={}|macroType={}|ox={}|oy={}]\n",
            m.name.replace(['\n', '\r', ']', '|'], ""),
            m.profile.start_delay_ms,
            m.profile.shortcut,
            m.profile.delay_time,
            m.profile.rand_min,
            m.profile.rand_max,
            match m.profile.delay_mode {
                DelayMode::Recorded => "recorded",
                DelayMode::Fixed => "fixed",
                DelayMode::Random => "random",
                DelayMode::None => "none",
            },
            move_mode_str(m.profile.move_mode),
            m.profile.macro_type,
            m.origin.0,
            m.origin.1
        ));
        for e in &m.events {
            s.push_str(&format!("{},{},{},{}\n", e.kind, e.a, e.b, e.c));
        }
    }
    fs::write(path(), s)
}

pub fn store_path() -> String {
    path().to_string_lossy().to_string()
}

pub fn hotkey_map(list: &[Macro]) -> Vec<(u8, usize)> {
    list.iter()
        .enumerate()
        .filter(|(_, m)| m.profile.shortcut != 0)
        .map(|(i, m)| (m.profile.shortcut, i))
        .collect()
}
