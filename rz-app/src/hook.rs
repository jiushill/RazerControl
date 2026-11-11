//! 软件按键映射：WH_MOUSE_LL 低级鼠标钩子 + SendInput 合成输入。
//!
//! Synapse 的按键映射同样是纯软件实现（其 Electron 内含 mapping_engine），
//! 因此这里用同样的思路，不需要动设备固件。

use crate::backend::Req;
use crate::config::Action;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Mutex,
};
use std::thread;

const WH_MOUSE_LL: i32 = 14;
const WH_KEYBOARD_LL: i32 = 13;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_RBUTTONDOWN: u32 = 0x0204;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_MOUSEWHEEL: u32 = 0x020A;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_SYSKEYUP: u32 = 0x0105;
const LLKHF_INJECTED: u32 = 0x10;
const WM_MBUTTONDOWN: u32 = 0x0207;
const WM_MBUTTONUP: u32 = 0x0208;
const WM_XBUTTONDOWN: u32 = 0x020B;
const WM_XBUTTONUP: u32 = 0x020C;
const LLMHF_INJECTED: u32 = 0x0001;

const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
const MOUSEEVENTF_MIDDLEDOWN: u32 = 0x0020;
const MOUSEEVENTF_MIDDLEUP: u32 = 0x0040;
const MOUSEEVENTF_XDOWN: u32 = 0x0080;
const MOUSEEVENTF_XUP: u32 = 0x0100;
const KEYEVENTF_KEYUP: u32 = 0x0002;

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct MsllHookStruct {
    pt: Point,
    mouse_data: u32,
    flags: u32,
    time: u32,
    extra: usize,
}

#[repr(C)]
struct KbdHookStruct {
    vk_code: u32,
    scan_code: u32,
    flags: u32,
    time: u32,
    extra: usize,
}

#[repr(C)]
struct Msg {
    hwnd: isize,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: Point,
    private: u32,
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

#[link(name = "user32")]
extern "system" {
    fn SetWindowsHookExW(
        id: i32,
        proc: Option<unsafe extern "system" fn(i32, usize, isize) -> isize>,
        module: isize,
        thread: u32,
    ) -> isize;
    fn CallNextHookEx(hook: isize, code: i32, w: usize, l: isize) -> isize;
    fn UnhookWindowsHookEx(hook: isize) -> i32;
    fn GetMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
    fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
    fn MapVirtualKeyW(code: u32, map_type: u32) -> u32;
}

const MAPVK_VK_TO_VSC: u32 = 0;
const LLKHF_EXTENDED: u32 = 0x01;

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
}

struct HookState {
    enabled: bool,
    actions: [Action; 5],
    /// 宏快照（绑定改动时同步）
    macros: Vec<crate::macros::Macro>,
    /// vk -> 宏序号
    hotkeys: Vec<(u8, usize)>,
    /// 正在等待用户按下快捷键
    capture: bool,
    /// 「开始/停止录制」的全局热键（0 = 未设置）
    record_key: u8,
    /// 热键需要的修饰键（MOD_* 位掩码；0 = 无修饰）
    record_mods: u8,
    /// 用热键开始录制时，是否记录鼠标移动
    record_track_moves: bool,
    tx: Option<Sender<Req>>,
    swallowed: [bool; 5],
}

static STATE: Mutex<Option<HookState>> = Mutex::new(None);
static INSTALLED: AtomicBool = AtomicBool::new(false);

fn dirs_state() -> Option<String> {
    std::env::var("APPDATA")
        .ok()
        .map(|p| format!("{p}\\RazerControl"))
}

extern "system" {
    fn GetLastError() -> u32;
}

/// 取钩子状态锁：忽略中毒，否则一旦有线程 panic，
/// 快捷键捕获/映射会静默全部失效（表现为"按什么键都没反应"）。
fn state_lock() -> std::sync::MutexGuard<'static, Option<HookState>> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}


/// 默认忽略注入事件（避免回放/自动化输入被录进去）。
/// 设环境变量 RC_ALLOW_INJECTED=1 可放行，便于自动化测试。
fn allow_injected() -> bool {
    use std::sync::OnceLock;
    static V: OnceLock<bool> = OnceLock::new();
    *V.get_or_init(|| {
        std::env::var("RC_ALLOW_INJECTED")
            .map(|v| v == "1")
            .unwrap_or(false)
    })
}

fn injected_blocked(flags: u32, mask: u32) -> bool {
    flags & mask != 0 && !allow_injected()
}

pub fn is_installed() -> bool {
    INSTALLED.load(Ordering::Relaxed)
}

/// 更新映射表（可在任意线程调用）。
pub fn configure(enabled: bool, actions: [Action; 5], tx: Sender<Req>) {
    { let mut g = state_lock();
        match g.as_mut() {
            Some(s) => {
                s.enabled = enabled;
                s.actions = actions;
                s.tx = Some(tx);
            }
            None => {
                *g = Some(HookState {
                    enabled,
                    actions,
                    macros: Vec::new(),
                    hotkeys: Vec::new(),
                    capture: false,
                    record_key: 0,
                    record_mods: 0,
                    record_track_moves: false,
                    tx: Some(tx),
                    swallowed: [false; 5],
                })
            }
        }
    }
}

/// 同步宏列表（录制/删除后调用），供 `Action::Macro` 回放使用。
pub fn set_macros(list: Vec<crate::macros::Macro>) {
    let keys = crate::macros::hotkey_map(&list);
    // 同步给宏引擎：宏套宏按名字查找
    crate::macros::set_library(&list);
    { let mut g = state_lock();
        if let Some(s) = g.as_mut() {
            s.macros = list;
            s.hotkeys = keys;
        }
    }
}

/// 开始/结束「按下一个键作为快捷键」的捕获
pub fn debug_log(msg: &str) {
    dbg(msg);
}

fn dbg_ts() -> u128 {
    static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    T0.get_or_init(std::time::Instant::now).elapsed().as_millis()
}

fn dbg(msg: &str) {
    if let Some(dir) = dirs_state() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("{dir}\\hook.log"))
        {
            let _ = writeln!(f, "[{:>7}ms] {msg}", dbg_ts());
        }
    }
}

/// 设置「开始/停止录制」热键
pub fn set_record_key(vk: u8, mods: u8, track_moves: bool) {
    crate::macros::set_record_hotkey(vk);
    let mut g = state_lock();
    if let Some(s) = g.as_mut() {
        s.record_key = vk;
        s.record_mods = mods;
        s.record_track_moves = track_moves;
    }
}

pub fn set_capture(on: bool) {
    dbg(&format!("set_capture({on})"));
    { let mut g = state_lock();
        if let Some(s) = g.as_mut() {
            s.capture = on;
        }
    }
}

static CAPTURED: std::sync::atomic::AtomicI16 = std::sync::atomic::AtomicI16::new(-1);

/// 取出捕获到的按键（取走后清零）
pub fn take_captured() -> Option<u8> {
    let v = CAPTURED.swap(-1, Ordering::Relaxed);
    if v >= 0 {
        Some(v as u8)
    } else {
        None
    }
}

/// 安装钩子（独立线程 + 消息循环），只需调用一次。
pub fn install() -> bool {
    if INSTALLED.load(Ordering::Relaxed) {
        return true;
    }
    thread::spawn(|| unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), module, 0);
        let mouse_err = if hook == 0 { GetLastError() } else { 0 };
        let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_proc), module, 0);
        let kb_err = if kb == 0 { GetLastError() } else { 0 };
        // 诊断日志：钩子装没装上、失败原因
        if let Some(dir) = dirs_state() {
            let _ = std::fs::write(
                format!("{dir}\\hook.log"),
                format!(
                    "mouse={:x} (err {}) kb={:x} (err {}) installed={}\n",
                    hook,
                    mouse_err,
                    kb,
                    kb_err,
                    hook != 0 && kb != 0
                ),
            );
        }
        if hook == 0 {
            return;
        }
        INSTALLED.store(true, Ordering::Relaxed);
        let mut msg: Msg = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {}
        UnhookWindowsHookEx(hook);
        if kb != 0 {
            UnhookWindowsHookEx(kb);
        }
        INSTALLED.store(false, Ordering::Relaxed);
    });
    true
}

unsafe fn send_mouse(flags_down: u32, flags_up: u32, data: u32) {
    let inputs = [
        Input {
            kind: 0,
            u: InputUnion {
                mi: MouseInput {
                    dx: 0,
                    dy: 0,
                    mouse_data: data,
                    flags: flags_down,
                    time: 0,
                    extra: 0,
                },
            },
        },
        Input {
            kind: 0,
            u: InputUnion {
                mi: MouseInput {
                    dx: 0,
                    dy: 0,
                    mouse_data: data,
                    flags: flags_up,
                    time: 0,
                    extra: 0,
                },
            },
        },
    ];
    SendInput(2, inputs.as_ptr(), std::mem::size_of::<Input>() as i32);
}

unsafe fn key_event(vk: u16, up: bool) {
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

unsafe fn key_tap(vk: u16) {
    key_event(vk, false);
    std::thread::sleep(std::time::Duration::from_millis(12));
    key_event(vk, true);
}

unsafe fn key_combo(modifier: u16, vk: u16) {
    key_event(modifier, false);
    key_event(vk, false);
    std::thread::sleep(std::time::Duration::from_millis(8));
    key_event(vk, true);
    key_event(modifier, true);
}

fn perform(act: Action, tx: Option<Sender<Req>>) {
    perform_with(act, tx, &[]);
}

fn perform_with(act: Action, tx: Option<Sender<Req>>, macros: &[crate::macros::Macro]) {
    unsafe {
        match act {
            Action::Pass | Action::Disabled => {}
            Action::Macro(k) => {
                if let Some(m) = macros.get(k as usize) {
                    crate::macros::play(m, 1.0);
                }
            }
            Action::Left => send_mouse(MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, 0),
            Action::Right => send_mouse(MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, 0),
            Action::Middle => send_mouse(MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, 0),
            Action::Back => send_mouse(MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, 1 << 16),
            Action::Forward => send_mouse(MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, 2 << 16),
            Action::Copy => key_combo(0x11, 0x43),
            Action::Paste => key_combo(0x11, 0x56),
            Action::PlayPause => key_tap(0xB3),
            Action::VolUp => key_tap(0xAF),
            Action::VolDown => key_tap(0xAE),
            Action::DpiCycle => {
                if let Some(t) = tx {
                    let _ = t.send(Req::CycleStage);
                }
            }
        }
    }
}

unsafe extern "system" fn kb_proc(code: i32, w: usize, l: isize) -> isize {
    if code >= 0 && std::env::var("RC_HOOK_TRACE").is_ok() {
        let info = &*(l as *const KbdHookStruct);
        dbg(&format!(
            "kb w=0x{:04X} vk=0x{:02X} flags=0x{:02X}",
            w,
            info.vk_code & 0xFF,
            info.flags
        ));
    }
    // 录制热键：按一下开始录，再按一下停止并交回 UI 保存
    if code >= 0 {
        let info = &*(l as *const KbdHookStruct);
        let is_down = matches!(w as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        // 注入事件（回放自己发出的按键）绝不能触发录制热键，
        // 否则宏里含 R 键时，"回放 R" 就会被当成"用户按了录制热键"。
        if is_down
            && info.flags & LLKHF_INJECTED == 0
            && !crate::macros::is_playing()
        {
            let vk = info.vk_code as u8;
            let mut hit = false;
            let mut track = false;
            if !crate::macros::is_modifier(vk) {
                let mods = crate::macros::mods_now();
                let g = state_lock();
                if let Some(st) = g.as_ref() {
                    if st.record_key != 0
                        && st.record_key == vk
                        && st.record_mods == mods
                        && !st.capture
                    {
                        hit = true;
                        track = st.record_track_moves;
                    }
                }
            }
            if hit {
                if crate::macros::is_recording() {
                    if let Some(r) = crate::macros::stop_recording() {
                        crate::macros::stash_finished(r);
                    }
                } else {
                    crate::macros::start_recording(track);
                }
                return 1; // 吞掉，别让这个键同时打到游戏里
            }
        }
    }

    if code >= 0 && crate::macros::is_recording() && !crate::macros::is_playing() {
        let info = &*(l as *const KbdHookStruct);
        if !injected_blocked(info.flags, LLKHF_INJECTED) {
            use crate::macros as m;
            let is_down = matches!(w as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            if is_down || matches!(w as u32, WM_KEYUP | WM_SYSKEYUP) {
                let vk = info.vk_code & 0xFF;
                let mut sc = MapVirtualKeyW(vk, MAPVK_VK_TO_VSC) as u16;
                if sc == 0 {
                    sc = vk as u16; // 兜底
                }
                if info.flags & LLKHF_EXTENDED != 0 {
                    sc |= m::SC_EXT;
                }
                m::push_key(sc, is_down);
            }
        }
    }

    // 快捷键：捕获模式吞掉这一键；否则匹配到就回放
    if code >= 0 {
        let info = &*(l as *const KbdHookStruct);
        if !injected_blocked(info.flags, LLKHF_INJECTED) {
            let is_down = matches!(w as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            if is_down {
                let vk = info.vk_code as u8;
                let mut triggered: Option<Vec<crate::macros::Macro>> = None;
                { let mut g = state_lock();
                    if let Some(st) = g.as_mut() {
                        if st.capture {
                            st.capture = false;
                            dbg(&format!("captured vk=0x{vk:02X}"));
                            CAPTURED.store(vk as i16, Ordering::Relaxed);
                            return 1; // 吞掉，避免这个键同时触发原功能
                        }
                        if let Some((_, idx)) = st.hotkeys.iter().find(|(k, _)| *k == vk) {
                            triggered = Some(st.macros.clone());
                            let i = *idx;
                            if let Some(m) = triggered.as_ref().and_then(|v| v.get(i)) {
                                crate::macros::play(m, 1.0);
                            }
                            triggered = None;
                        }
                    }
                }
                let _ = triggered;
            }
        }
    }

    CallNextHookEx(0, code, w, l)
}

unsafe extern "system" fn hook_proc(code: i32, w: usize, l: isize) -> isize {
    if code < 0 {
        return CallNextHookEx(0, code, w, l);
    }
    let info = &*(l as *const MsllHookStruct);
    let msg = w as u32;

    // ---- 宏录制（注入事件不录，否则回放会被再次录进去）----
    if !injected_blocked(info.flags, LLMHF_INJECTED)
        && crate::macros::is_recording()
        && !crate::macros::is_playing()
    {
        use crate::macros as m;
        match msg {
            WM_MOUSEMOVE => m::push_move(info.pt.x, info.pt.y),
            WM_LBUTTONDOWN => m::push_mouse(m::BTN_LEFT, true),
            WM_LBUTTONUP => m::push_mouse(m::BTN_LEFT, false),
            WM_RBUTTONDOWN => m::push_mouse(m::BTN_RIGHT, true),
            WM_RBUTTONUP => m::push_mouse(m::BTN_RIGHT, false),
            WM_MBUTTONDOWN => m::push_mouse(m::BTN_MIDDLE, true),
            WM_MBUTTONUP => m::push_mouse(m::BTN_MIDDLE, false),
            WM_XBUTTONDOWN | WM_XBUTTONUP => {
                let which = (info.mouse_data >> 16) & 0xFFFF;
                // 实测：靠前侧键 = XBUTTON2，靠后 = XBUTTON1
                let btn = if which == 2 { m::BTN_X1 } else { m::BTN_X2 };
                m::push_mouse(btn, msg == WM_XBUTTONDOWN);
            }
            WM_MOUSEWHEEL => {
                let delta = (info.mouse_data as i16) as i32;
                m::push_wheel(delta);
            }
            _ => {}
        }
    }

    if info.flags & LLMHF_INJECTED != 0 {
        return CallNextHookEx(0, code, w, l);
    }
    let (idx, is_down) = match msg {
        WM_MBUTTONDOWN => (2usize, true),
        WM_MBUTTONUP => (2, false),
        WM_XBUTTONDOWN | WM_XBUTTONUP => {
            let which = (info.mouse_data >> 16) & 0xFFFF;
            (if which == 2 { 3 } else { 4 }, msg == WM_XBUTTONDOWN)
        }
        _ => return CallNextHookEx(0, code, w, l),
    };

    let mut g = state_lock();
    let st = match g.as_mut() {
        Some(s) if s.enabled => s,
        _ => return CallNextHookEx(0, code, w, l),
    };
    let act = st.actions[idx];
    if is_down {
        if act == Action::Pass {
            return CallNextHookEx(0, code, w, l);
        }
        st.swallowed[idx] = true;
        let tx = st.tx.clone();
        let macros = st.macros.clone();
        drop(g);
        perform_with(act, tx, &macros);
        1
    } else if st.swallowed[idx] {
        st.swallowed[idx] = false;
        1
    } else {
        CallNextHookEx(0, code, w, l)
    }
}
