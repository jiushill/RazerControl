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
/// 动作步骤（雷云「鼠标功能」）：a = 动作码，见 ACTIONS
pub const T_ACTION: u8 = 5;
/// 按键步骤（雷云「键盘功能」）：a = scan code，回放时自动按下+抬起
pub const T_KEYPRESS: u8 = 6;
/// 文本步骤（雷云「文本功能」）：text 里是要打出来的文字
pub const T_TEXT: u8 = 7;
/// 启动步骤（雷云「启动」）：a=0 程序 / a=1 网站，text = 路径或网址
pub const T_LAUNCH: u8 = 8;
pub const LAUNCH_PROGRAM: i32 = 0;
pub const LAUNCH_URL: i32 = 1;
/// 运行命令步骤（雷云「运行命令」）：text = 命令行
pub const T_RUN: u8 = 9;
/// 宏步骤（雷云「宏」= 宏套宏）：text = 被引用宏的名字
pub const T_MACRO: u8 = 10;
/// 循环区间开始（a = 重复次数）；与 T_LOOP_END 成对出现
pub const T_LOOP_START: u8 = 11;
/// 循环区间结束
pub const T_LOOP_END: u8 = 12;
/// 雷云的文本上限
pub const TEXT_MAX: usize = 250;

/// 未选择任何动作（插入后的初始状态，对应雷云的「选择操作」）
pub const ACT_UNSET: i32 = -1;

/// 雷云的鼠标动作表（左键单击/右键单击/滚轮单击/鼠标按键4/5/双击/向左滚动…）
pub const ACTIONS: [(&str, &str); 10] = [
    ("左键单击", "LEFT_CLICK"),
    ("右键单击", "RIGHT_CLICK"),
    ("滚轮单击", "SCROLL_CLICK"),
    ("鼠标按键 4", "MOUSE_BUTTON_4"),
    ("鼠标按键 5", "MOUSE_BUTTON_5"),
    ("双击", "DOUBLE_CLICK"),
    ("向左滚动", "SCROLL_LEFT"),
    ("向右滚动", "SCROLL_RIGHT"),
    ("向上滚动", "SCROLL_UP"),
    ("向下滚动", "SCROLL_DOWN"),
];

pub fn action_name(code: i32) -> &'static str {
    if code < 0 || code as usize >= ACTIONS.len() {
        "选择操作"
    } else {
        ACTIONS[code as usize].0
    }
}

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

#[derive(Clone, Debug, Default)]
pub struct Ev {
    pub kind: u8,
    /// delay: 毫秒；keyboard: scancode；mouse: 键号；wheel: 增量
    pub a: i32,
    /// keyboard/mouse: flag；mouse_move: y
    pub b: i32,
    /// mouse_move: x
    pub c: i32,
    /// 仅 T_TEXT 使用：要打出来的文字
    pub text: String,
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
            text: String::new(),
        }
    }
    pub fn mouse(btn: u8, down: bool) -> Self {
        Ev {
            kind: T_MOUSE,
            a: btn as i32,
            b: if down { FLAG_DOWN as i32 } else { FLAG_UP as i32 },
            c: 0,
            text: String::new(),
        }
    }
    /// 动作步骤（雷云「鼠标功能」）：code = ACTIONS 下标，ACT_UNSET 表示未选择
    pub fn action(code: i32) -> Self {
        Ev {
            kind: T_ACTION,
            a: code,
            ..Default::default()
        }
    }
    /// 循环开始（次数存在 a，至少 1）
    pub fn loop_start(times: u32) -> Self {
        Ev {
            kind: T_LOOP_START,
            a: times.max(1) as i32,
            ..Default::default()
        }
    }
    /// 循环结束
    pub fn loop_end() -> Self {
        Ev {
            kind: T_LOOP_END,
            ..Default::default()
        }
    }
    /// 宏步骤（雷云「宏」= 宏套宏）
    pub fn macro_ref(name: &str) -> Self {
        Ev {
            kind: T_MACRO,
            text: name.to_string(),
            ..Default::default()
        }
    }
    /// 运行命令步骤（雷云「运行命令」）
    pub fn run(cmd: &str) -> Self {
        Ev {
            kind: T_RUN,
            text: cmd.to_string(),
            ..Default::default()
        }
    }
    /// 启动步骤（雷云「启动」）
    pub fn launch(kind: i32, target: &str) -> Self {
        Ev {
            kind: T_LAUNCH,
            a: kind,
            text: target.to_string(),
            ..Default::default()
        }
    }
    /// 文本步骤（雷云「文本功能」）
    pub fn text_step(s: &str) -> Self {
        Ev {
            kind: T_TEXT,
            text: s.chars().take(TEXT_MAX).collect(),
            ..Default::default()
        }
    }
    /// 按键步骤（雷云「键盘功能」）：sc = scan code，回放时自动按下+抬起
    pub fn keypress(sc: i32) -> Self {
        Ev {
            kind: T_KEYPRESS,
            a: sc,
            ..Default::default()
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
            text: String::new(),
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
/// 当前的「开始/停止录制」热键（VK），用于把热键自身的按下/抬起从录制结果里剔除
static RECORD_HOTKEY: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub fn set_record_hotkey(vk: u8) {
    RECORD_HOTKEY.store(vk, std::sync::atomic::Ordering::Relaxed);
}
static PLAYING: Mutex<bool> = Mutex::new(false);

/// 取录制锁：即使之前有线程 panic 污染了锁也照常使用，
/// 否则录制会静默失效（界面显示"录制中"但一步都记不到）。
fn rec_lock() -> std::sync::MutexGuard<'static, Option<Rec>> {
    REC.lock().unwrap_or_else(|e| e.into_inner())
}

fn playing_lock() -> std::sync::MutexGuard<'static, bool> {
    PLAYING.lock().unwrap_or_else(|e| e.into_inner())
}

static PENDING: Mutex<Option<(Vec<Ev>, (i32, i32))>> = Mutex::new(None);

/// 热键停止录制后，把结果暂存起来交给 UI 线程存盘
pub fn stash_finished(r: (Vec<Ev>, (i32, i32))) {
    let mut g = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    *g = Some(r);
}

pub fn take_finished() -> Option<(Vec<Ev>, (i32, i32))> {
    let mut g = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    g.take()
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
    crate::hook::debug_log(&format!("start_recording track={track_moves}"));
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
        strip_hotkey_edges(&mut r.events);
        crate::hook::debug_log(&format!("stop_recording events={}", r.events.len()));
        (r.events, r.origin)
    })
}

/// 录制热键自身（含修饰键）不该出现在宏里。
/// 组合键 Ctrl+Alt+R 会留下：开头的 R 抬起 / Alt 抬起 / Ctrl 抬起，
/// 结尾的 Ctrl 按下 / Alt 按下 / R 按下 —— 这些都要剔掉。
fn strip_hotkey_edges(events: &mut Vec<Ev>) {
    let vk = RECORD_HOTKEY.load(std::sync::atomic::Ordering::Relaxed);
    if vk == 0 {
        return;
    }
    let hot_sc = vk_to_scancode(vk) & 0x1FFF;
    // 修饰键的扫描码（含扩展位形式）
    const MOD_SC: [u16; 8] = [0x1D, 0x2A, 0x36, 0x38, 0x5B, 0x5C, 0xE01D, 0xE038];

    let is_hot_or_mod = |e: &Ev| {
        if e.kind != T_KEYBOARD {
            return false;
        }
        let sc = e.a.max(0) as u16 & 0x1FFF;
        sc == hot_sc || MOD_SC.iter().any(|m| (*m & 0x1FFF) == sc)
    };

    // 开头：直到遇到第一个"真实动作"，把热键/修饰键的抬起都去掉
    loop {
        let mut i = 0;
        while i < events.len() && events[i].kind == T_DELAY {
            i += 1;
        }
        if i < events.len()
            && is_hot_or_mod(&events[i])
            && events[i].b as u8 == FLAG_UP
        {
            events.drain(0..=i);
            continue;
        }
        break;
    }

    // 结尾：热键/修饰键的按下（比如按 Ctrl+Alt+R 停止时，Ctrl/Alt/R 的按下）
    loop {
        let mut i = events.len();
        while i > 0 && events[i - 1].kind == T_DELAY {
            i -= 1;
        }
        if i > 0 && is_hot_or_mod(&events[i - 1]) && events[i - 1].b as u8 == FLAG_DOWN {
            events.truncate(i - 1);
            while events.last().map(|e| e.kind == T_DELAY).unwrap_or(false) {
                events.pop();
            }
            continue;
        }
        break;
    }
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
/// 相对位移事件（Raw Input 录下来的 dx/dy）：a=1 标记，c=dx，b=dy
pub fn push_move_rel(dx: i32, dy: i32) {
    if dx == 0 && dy == 0 {
        return;
    }
    if let Ok(mut g) = REC.lock() {
        if let Some(r) = g.as_mut() {
            if !r.track_moves {
                return;
            }
            let now = Instant::now();
            let dt = now.duration_since(r.last).as_millis().min(60_000) as u32;
            r.last = now;
            r.events.push(Ev::delay(dt));
            r.events.push(Ev {
                kind: T_MOUSE_MOVE,
                a: 1,
                b: dy,
                c: dx,
                text: String::new(),
            });
        }
    }
}

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
fn send_input(inp: &Input) {
    unsafe {
        SendInput(1, inp, std::mem::size_of::<Input>() as i32);
    }
}

fn mouse_ev(flags: u32, data: u32) {
    let inp = Input {
        kind: 0,
        u: InputUnion {
            mi: MouseInput {
                dx: 0,
                dy: 0,
                mouse_data: data,
                flags,
                time: 0,
                extra: 0,
            },
        },
    };
    send_input(&inp);
}

/// 0=左 1=右 2=中 3=X1 4=X2
fn mouse_click(btn: u8) {
    let (down, up, data) = match btn {
        1 => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, 0),
        2 => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, 0),
        3 => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, 1),
        4 => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, 2),
        _ => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, 0),
    };
    mouse_ev(down, data);
    std::thread::sleep(std::time::Duration::from_millis(20));
    mouse_ev(up, data);
}

const MOUSEEVENTF_HWHEEL: u32 = 0x1000;

fn mouse_wheel(vertical: bool, delta: i32) {
    let flag = if vertical {
        MOUSEEVENTF_WHEEL
    } else {
        MOUSEEVENTF_HWHEEL
    };
    mouse_ev(flag, delta as u32);
}

fn key_event(sc: u16, down: bool) {
    let mut flags = 0x0008; // KEYEVENTF_SCANCODE
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    if sc & SC_EXT != 0 {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    let inp = Input {
        kind: 1,
        u: InputUnion {
            ki: KeybdInput {
                vk: 0,
                scan: sc & 0x1FFF,
                flags,
                time: 0,
                extra: 0,
            },
        },
    };
    send_input(&inp);
}

/// 执行一条命令（cmd /C，无窗口，不等待返回）
fn run_command(cmd: &str) {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return;
    }
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // 必须用 raw_arg：Rust 默认会对参数做引号转义（" → \"），
    // cmd.exe 不认这种转义，带引号的命令（start "" "C:\...\chrome.exe"）就会失败。
    let _ = std::process::Command::new("cmd")
        .raw_arg(format!("/C {cmd}"))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
    crate::hook::debug_log(&format!("run_command: {cmd}"));
}

/// 打开一个程序或网址（ShellExecuteW "open" 两者通用）
fn open_target(target: &str) {
    if target.trim().is_empty() {
        return;
    }
    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: isize,
            op: *const u16,
            file: *const u16,
            params: *const u16,
            dir: *const u16,
            show: i32,
        ) -> isize;
    }
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    let file: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        ShellExecuteW(0, verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), 1);
    }
}

/// 「浏览…」选择可执行文件（启动-程序）
pub fn pick_file() -> Option<String> {
    #[repr(C)]
    struct OpenFileNameW {
        l_struct_size: u32,
        hwnd_owner: isize,
        h_instance: isize,
        lpstr_filter: *const u16,
        lpstr_custom_filter: *mut u16,
        n_max_cust_filter: u32,
        n_filter_index: u32,
        lpstr_file: *mut u16,
        n_max_file: u32,
        lpstr_file_title: *mut u16,
        n_max_file_title: u32,
        lpstr_initial_dir: *const u16,
        lpstr_title: *const u16,
        flags: u32,
        n_file_offset: u16,
        n_file_extension: u16,
        lpstr_def_ext: *const u16,
        l_cust_data: isize,
        lpfn_hook: isize,
        lp_template_name: *const u16,
        pv_reserved: *mut std::ffi::c_void,
        dw_reserved: u32,
        flags_ex: u32,
    }
    #[link(name = "comdlg32")]
    extern "system" {
        fn GetOpenFileNameW(ofn: *mut OpenFileNameW) -> i32;
    }
    let filter: Vec<u16> = "程序 (*.exe)\0*.exe\0所有文件 (*.*)\0*.*\0\0"
        .encode_utf16()
        .collect();
    let title: Vec<u16> = "选择要启动的程序\0".encode_utf16().collect();
    let mut buf = vec![0u16; 1024];
    let mut ofn: OpenFileNameW = unsafe { std::mem::zeroed() };
    ofn.l_struct_size = std::mem::size_of::<OpenFileNameW>() as u32;
    ofn.lpstr_filter = filter.as_ptr();
    ofn.lpstr_file = buf.as_mut_ptr();
    ofn.n_max_file = buf.len() as u32;
    ofn.lpstr_title = title.as_ptr();
    ofn.flags = 0x0000_1000 | 0x0000_0800 | 0x0000_0004; // FILEMUSTEXIST | PATHMUSTEXIST | HIDEREADONLY
    let ok = unsafe { GetOpenFileNameW(&mut ofn) };
    if ok == 0 {
        return None;
    }
    let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..end]))
}

/// 把一段文字打出来：ASCII 用扫描码（游戏也认），其它字符用 Unicode 注入
/// 把一段文字打出来。
///
/// 统一用 KEYEVENTF_UNICODE 注入：直接产生字符，不经过键盘布局和输入法。
/// 之前 ASCII 走 scancode + VkKeyScan，中文输入法会把 "r" 拦成候选字（如「让」），
/// 还会吃掉回车，导致宏在浏览器/聊天框里打不出原样文字。
fn type_text(s: &str) {
    for ch in s.chars() {
        let mut buf = [0u16; 2];
        for u in ch.encode_utf16(&mut buf) {
            let mut down: Input = unsafe { std::mem::zeroed() };
            down.kind = 1;
            down.u.ki.scan = *u;
            down.u.ki.flags = 0x0004; // KEYEVENTF_UNICODE
            send_input(&down);

            let mut up: Input = unsafe { std::mem::zeroed() };
            up.kind = 1;
            up.u.ki.scan = *u;
            up.u.ki.flags = 0x0004 | 0x0002;
            send_input(&up);
            std::thread::sleep(std::time::Duration::from_millis(6));
        }
    }
}

/// 执行一个雷云动作码（见 ACTIONS）
fn play_action(code: i32, _factor: f32) {
    match code {
        0 => mouse_click(0),
        1 => mouse_click(1),
        2 => mouse_click(2),
        3 => mouse_click(3),
        4 => mouse_click(4),
        5 => {
            mouse_click(0);
            std::thread::sleep(std::time::Duration::from_millis(60));
            mouse_click(0);
        }
        6 => mouse_wheel(false, -120),
        7 => mouse_wheel(false, 120),
        8 => mouse_wheel(true, 120),
        9 => mouse_wheel(true, -120),
        _ => {}
    }
}

pub fn play(m: &Macro, speed: f32) {
    play_inner(m, speed, 0);
}

/// 宏库里按名字找（供「宏」步骤引用其它宏）
pub fn find_macro(name: &str) -> Option<Macro> {
    LIB.lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|m| m.name == name)
        .cloned()
}

/// 同步宏库（App 保存时调用），供宏套宏查找
pub fn set_library(list: &[Macro]) {
    let mut g = LIB.lock().unwrap_or_else(|e| e.into_inner());
    *g = list.to_vec();
}

static LIB: Mutex<Vec<Macro>> = Mutex::new(Vec::new());

fn play_inner(m: &Macro, speed: f32, depth: u32) {
    crate::hook::debug_log(&format!(
        "play start name={} events={}",
        m.name,
        m.events.len()
    ));
    if m.events.is_empty() {
        return;
    }
    // 顶层回放才抢占 PLAYING；嵌套（宏套宏）直接进入，否则会被自己的门闸挡住
    if depth == 0 {
        let mut p = playing_lock();
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

        // 索引式遍历（为了支持「循环开始/结束」区间跳转）
        let mut i = 0usize;
        let mut loops: Vec<(usize, u32)> = Vec::new();
        while i < evs.len() {
            let e = &evs[i];
            match e.kind {
                T_LOOP_START => {
                    let n = e.a.max(1) as u32;
                    if n > 1 {
                        loops.push((i, n - 1));
                    }
                    i += 1;
                    continue;
                }
                T_LOOP_END => {
                    if let Some((start, rem)) = loops.last_mut() {
                        if *rem > 0 {
                            *rem -= 1;
                            crate::hook::debug_log(&format!("loop rewind, rem={}", *rem));
                            let s = *start;
                            i = s + 1;
                            continue;
                        }
                        loops.pop();
                    }
                    i += 1;
                    continue;
                }
                _ => {}
            }
            if ev_disabled(e) {
                i += 1;
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
                    T_ACTION => {
                        play_action(e.a, factor);
                    }
                    T_LAUNCH => {
                        open_target(&e.text);
                    }
                    T_MACRO => {
                        // 宏套宏：按名字引用另一个宏，最多嵌套 8 层防止互相引用死循环
                        if depth < 8 {
                            if let Some(sub) = find_macro(&e.text) {
                                play_inner(&sub, factor, depth + 1);
                            }
                        }
                    }
                    T_RUN => {
                        run_command(&e.text);
                    }
                    T_TEXT => {
                        type_text(&e.text);
                    }
                    T_KEYPRESS => {
                        let sc = e.a.max(0) as u16;
                        key_event(sc, true);
                        std::thread::sleep(std::time::Duration::from_millis(40));
                        key_event(sc, false);
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
                        // Raw Input 录制的相对增量：原样注入（游戏 raw input 直接生效）
                        _ if e.a == 1 => {
                            inject_mouse(MOUSEEVENTF_MOVE, 0, e.c, e.b);
                        }
                        _ => {
                            let (px, py) = prev.unwrap_or_else(|| {
                                let mut cur = (0i32, 0i32);
                                GetCursorPos(&mut cur as *mut (i32, i32));
                                cur
                            });
                            let dx = e.c - px;
                            let dy = e.b - py;
                            // 1) 相对位移注入：游戏（CS2 等 raw input）只认这个，
                            //    SetCursorPos 在游戏里是无效的
                            if dx != 0 || dy != 0 {
                                inject_mouse(MOUSEEVENTF_MOVE, 0, dx, dy);
                            }
                            // 2) 绝对模式再补一次 SetCursorPos，保证桌面光标也到位
                            if p.move_mode == MoveMode::Absolute {
                                SetCursorPos(e.c, e.b);
                            }
                            prev = Some((e.c, e.b));
                        }
                    },
                    _ => {}
                }
            }
            i += 1;
        }
        if depth == 0 {
            let mut p = playing_lock();
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
    let fixed = match vk {
        0x08 => "退格",
        0x09 => "Tab",
        0x0D => "回车",
        0x10 => "Shift",
        0x11 => "Ctrl",
        0x12 => "Alt",
        0x13 => "Pause",
        0x14 => "CapsLock",
        0x1B => "Esc",
        0x20 => "空格",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "←",
        0x26 => "↑",
        0x27 => "→",
        0x28 => "↓",
        0x2C => "PrtSc",
        0x2D => "Insert",
        0x2E => "Delete",
        0x5B => "左Win",
        0x5C => "右Win",
        0x5D => "菜单键",
        0x90 => "NumLock",
        0x91 => "ScrollLock",
        0xA0 => "左Shift",
        0xA1 => "右Shift",
        0xA2 => "左Ctrl",
        0xA3 => "右Ctrl",
        0xA4 => "左Alt",
        0xA5 => "右Alt",
        // OEM 符号键
        0xBA => ";",
        0xBB => "=",
        0xBC => ",",
        0xBD => "-",
        0xBE => ".",
        0xBF => "/",
        0xC0 => "`",
        0xDB => "[",
        0xDC => "\\",
        0xDD => "]",
        0xDE => "'",
        // 小键盘
        0x60..=0x69 => return format!("小键盘 {}", vk - 0x60),
        0x6A => "小键盘 *",
        0x6B => "小键盘 +",
        0x6D => "小键盘 -",
        0x6E => "小键盘 .",
        0x6F => "小键盘 /",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_string();
    }
    match vk {
        0x30..=0x39 => ((vk as u8) as char).to_string(),
        0x41..=0x5A => ((vk as u8) as char).to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
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
    if e.kind == T_ACTION {
        return action_name(e.a).to_string();
    }
    if e.kind == T_KEYPRESS {
        return format!("{} 按键", sc_name(e.a.max(0) as u16));
    }
    if e.kind == T_MOUSE_MOVE && e.a == 1 {
        return format!("移动 ({}, {})", e.c, e.b);
    }
    if e.kind == T_LOOP_START {
        return format!("循环 {} 次", e.a.max(1));
    }
    if e.kind == T_LOOP_END {
        return "循环结束".to_string();
    }
    if e.kind == T_MACRO {
        let n = if e.text.is_empty() {
            "未选择".to_string()
        } else {
            e.text.clone()
        };
        return format!("宏：{}", n);
    }
    if e.kind == T_RUN {
        let mut t: String = e.text.chars().take(40).collect();
        if e.text.chars().count() > 40 {
            t.push('…');
        }
        return format!("运行命令：{}", t);
    }
    if e.kind == T_LAUNCH {
        let kind = if e.a == LAUNCH_URL { "网站" } else { "程序" };
        let mut t: String = e.text.chars().take(28).collect();
        if e.text.chars().count() > 28 {
            t.push('…');
        }
        if t.is_empty() {
            t = "未设置".to_string();
        }
        return format!("{}：{}", kind, t);
    }
    if e.kind == T_TEXT {
        let preview: String = e.text.chars().take(24).collect();
        let ell = if e.text.chars().count() > 24 { "…" } else { "" };
        return format!("文本：{}{}", preview, ell);
    }
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
        let p: Vec<&str> = line.splitn(5, ',').collect();
        if p.len() < 4 || out.is_empty() {
            continue;
        }
        let kind = p[0].parse::<u8>().ok();
        let a = p[1].parse::<i32>().ok();
        let b = p[2].parse::<i32>().ok();
        let c = p[3].parse::<i32>().ok();
        if let (Some(kind), Some(a), Some(b), Some(c)) = (kind, a, b, c) {
            let ev_text = if p.len() >= 5 { unescape_text(p[4]) } else { String::new() };
            if let Some(m) = out.last_mut() {
                m.events.push(Ev {
                    kind,
                    a,
                    b,
                    c,
                    text: ev_text,
                });
            }
        }
    }
    out
}

fn escape_text(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "")
        .replace('|', "\\p")
}

fn unescape_text(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some('p') => out.push('|'),
                Some('\\') => out.push('\\'),
                // 未知转义：原样保留反斜杠（手写单反斜杠的 Windows 路径不能被吃掉）
                Some(o) => {
                    out.push('\\');
                    out.push(o);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
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
            // 带字符串的步骤（文本 / 启动 / 运行命令）多写一列，其余保持 4 列
            if e.kind == T_TEXT || e.kind == T_LAUNCH || e.kind == T_RUN || e.kind == T_MACRO {
                s.push_str(&format!(
                    "{},{},{},{},{}\n",
                    e.kind,
                    e.a,
                    e.b,
                    e.c,
                    escape_text(&e.text)
                ));
            } else {
                s.push_str(&format!("{},{},{},{}\n", e.kind, e.a, e.b, e.c));
            }
        }
    }
    fs::write(path(), s)
}

pub fn store_path() -> String {
    path().to_string_lossy().to_string()
}



// ---------- 热键组合键 ----------
pub const MOD_CTRL: u8 = 1;
pub const MOD_SHIFT: u8 = 2;
pub const MOD_ALT: u8 = 4;
pub const MOD_WIN: u8 = 8;

/// 这个 VK 是不是修饰键本身（单独按不算热键）
pub fn is_modifier(vk: u8) -> bool {
    matches!(
        vk,
        0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0 | 0xA1 | 0xA2 | 0xA3 | 0xA4 | 0xA5
    )
}

/// 当前按住的修饰键（钩子里调用，判断组合键）
pub fn mods_now() -> u8 {
    extern "system" {
        fn GetAsyncKeyState(vk: i32) -> i16;
    }
    let down = |vk: i32| unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 };
    let mut m = 0u8;
    if down(0x11) || down(0xA2) || down(0xA3) {
        m |= MOD_CTRL;
    }
    if down(0x10) || down(0xA0) || down(0xA1) {
        m |= MOD_SHIFT;
    }
    if down(0x12) || down(0xA4) || down(0xA5) {
        m |= MOD_ALT;
    }
    if down(0x5B) || down(0x5C) {
        m |= MOD_WIN;
    }
    m
}

/// 组合键显示名，例如 "Ctrl+Alt+R"
pub fn combo_name(vk: u8, mods: u8) -> String {
    if vk == 0 {
        return "未设置".to_string();
    }
    let mut s = String::new();
    if mods & MOD_CTRL != 0 {
        s.push_str("Ctrl+");
    }
    if mods & MOD_SHIFT != 0 {
        s.push_str("Shift+");
    }
    if mods & MOD_ALT != 0 {
        s.push_str("Alt+");
    }
    if mods & MOD_WIN != 0 {
        s.push_str("Win+");
    }
    s.push_str(&key_name(vk));
    s
}

/// egui 的按键 -> Windows VK（窗口有焦点时的捕获/触发通道）
pub fn egui_key_vk(k: egui::Key) -> Option<u8> {
    use egui::Key as K;
    Some(match k {
        K::A => 0x41, K::B => 0x42, K::C => 0x43, K::D => 0x44, K::E => 0x45,
        K::F => 0x46, K::G => 0x47, K::H => 0x48, K::I => 0x49, K::J => 0x4A,
        K::K => 0x4B, K::L => 0x4C, K::M => 0x4D, K::N => 0x4E, K::O => 0x4F,
        K::P => 0x50, K::Q => 0x51, K::R => 0x52, K::S => 0x53, K::T => 0x54,
        K::U => 0x55, K::V => 0x56, K::W => 0x57, K::X => 0x58, K::Y => 0x59,
        K::Z => 0x5A,
        K::Num0 => 0x30, K::Num1 => 0x31, K::Num2 => 0x32, K::Num3 => 0x33,
        K::Num4 => 0x34, K::Num5 => 0x35, K::Num6 => 0x36, K::Num7 => 0x37,
        K::Num8 => 0x38, K::Num9 => 0x39,
        K::F1 => 0x70, K::F2 => 0x71, K::F3 => 0x72, K::F4 => 0x73,
        K::F5 => 0x74, K::F6 => 0x75, K::F7 => 0x76, K::F8 => 0x77,
        K::F9 => 0x78, K::F10 => 0x79, K::F11 => 0x7A, K::F12 => 0x7B,
        K::Space => 0x20, K::Enter => 0x0D, K::Tab => 0x09,
        K::Escape => 0x1B, K::Backspace => 0x08, K::Delete => 0x2E,
        K::Insert => 0x2D, K::Home => 0x24, K::End => 0x23,
        K::PageUp => 0x21, K::PageDown => 0x22,
        K::ArrowLeft => 0x25, K::ArrowUp => 0x26,
        K::ArrowRight => 0x27, K::ArrowDown => 0x28,
        K::Minus => 0xBD, K::Plus | K::Equals => 0xBB,
        K::Comma => 0xBC, K::Period => 0xBE, K::Slash => 0xBF,
        K::Semicolon => 0xBA, K::Quote => 0xDE,
        K::OpenBracket => 0xDB, K::CloseBracket => 0xDD,
        K::Backslash => 0xDC, K::Backtick => 0xC0,
        _ => return None,
    })
}

/// 生成不与现有宏重名的新名字：取「宏 N」中最小的未使用 N。
/// 之前用 len+1，删掉一个再新建就会撞名（出现两个「宏 3」）。
pub fn unique_name(list: &[Macro]) -> String {
    for n in 1..=9999 {
        let cand = format!("宏 {}", n);
        if !list.iter().any(|m| m.name == cand) {
            return cand;
        }
    }
    format!("宏 {}", list.len() + 1)
}

pub fn hotkey_map(list: &[Macro]) -> Vec<(u8, usize)> {
    list.iter()
        .enumerate()
        .filter(|(_, m)| m.profile.shortcut != 0)
        .map(|(i, m)| (m.profile.shortcut, i))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(sc: u16, down: bool) -> Ev {
        Ev::key(sc, down)
    }

    #[test]
    fn strips_hotkey_edges() {
        set_record_hotkey(0x52); // R
        let sc_r = vk_to_scancode(0x52);
        let sc_a = vk_to_scancode(0x41);

        // 开始：R/Alt/Ctrl 的抬起；中间：正常 A 键；结尾：Ctrl/Alt/R 的按下
        let mut evs = vec![
            Ev::delay(0),
            k(sc_r, false),
            Ev::delay(3),
            k(0x38, false), // Alt 抬起
            Ev::delay(3),
            k(0x1D, false), // Ctrl 抬起
            Ev::delay(500),
            k(sc_a, true),
            Ev::delay(80),
            k(sc_a, false),
            Ev::delay(120),
            k(0x1D, true), // Ctrl 按下
            Ev::delay(2),
            k(0x38, true), // Alt 按下
            Ev::delay(2),
            k(sc_r, true),
        ];
        strip_hotkey_edges(&mut evs);

        // 热键的按下/抬起都该消失，A 键动作保留
        assert_eq!(evs.iter().filter(|e| e.kind == T_KEYBOARD).count(), 2);
        assert!(evs.iter().all(|e| e.kind != T_KEYBOARD
            || ((e.a.max(0) as u16) & 0x1FFF) == (sc_a & 0x1FFF)));
        set_record_hotkey(0);
    }

    #[test]
    fn text_step_roundtrip() {
        let raw = "Raid|EnMei\n第二行\\结尾";
        let esc = escape_text(raw);
        assert_eq!(unescape_text(&esc), raw);

        let ev = Ev::text_step("你好abc|d");
        assert_eq!(ev.kind, T_TEXT);
        assert_eq!(ev.text, "你好abc|d");
        assert!(ev_label(&ev).starts_with("文本："));
    }

    #[test]
    fn text_is_capped() {
        let long: String = "a".repeat(400);
        let ev = Ev::text_step(&long);
        assert_eq!(ev.text.chars().count(), TEXT_MAX);
    }

    #[test]
    fn unique_names_never_collide() {
        let mk = |n: &str| {
            let mut m = Macro::default();
            m.name = n.into();
            m
        };
        // 空列表 -> 宏 1
        assert_eq!(unique_name(&[]), "宏 1");
        // 已有 1、2、3 -> 宏 4
        assert_eq!(unique_name(&[mk("宏 1"), mk("宏 2"), mk("宏 3")]), "宏 4");
        // 删掉 宏 1 后（剩 2、3）-> 宏 1（补空缺，不撞名）
        assert_eq!(unique_name(&[mk("宏 2"), mk("宏 3")]), "宏 1");
        // 中间有空洞 -> 取最小空位
        assert_eq!(unique_name(&[mk("宏 1"), mk("宏 3")]), "宏 2");
    }

    #[test]
    fn macro_library_lookup() {
        let mut a = Macro::default();
        a.name = "A".into();
        let mut b = Macro::default();
        b.name = "B".into();
        set_library(&[a, b]);
        assert!(find_macro("A").is_some());
        assert!(find_macro("B").is_some());
        assert!(find_macro("C").is_none());

        // 宏步骤的标签与引用
        let ev = Ev::macro_ref("A");
        assert_eq!(ev.kind, T_MACRO);
        assert!(ev_label(&ev).contains("A"));
        set_library(&[]);
    }

    #[test]
    fn keeps_everything_without_hotkey() {
        set_record_hotkey(0);
        let mut evs = vec![Ev::delay(10), k(30, true), Ev::delay(50), k(30, false)];
        let before = evs.len();
        strip_hotkey_edges(&mut evs);
        assert_eq!(evs.len(), before);
    }
}
