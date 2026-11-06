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
    tx: Option<Sender<Req>>,
    swallowed: [bool; 5],
}

static STATE: Mutex<Option<HookState>> = Mutex::new(None);
static INSTALLED: AtomicBool = AtomicBool::new(false);

pub fn is_installed() -> bool {
    INSTALLED.load(Ordering::Relaxed)
}

/// 更新映射表（可在任意线程调用）。
pub fn configure(enabled: bool, actions: [Action; 5], tx: Sender<Req>) {
    if let Ok(mut g) = STATE.lock() {
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
    if let Ok(mut g) = STATE.lock() {
        if let Some(s) = g.as_mut() {
            s.macros = list;
            s.hotkeys = keys;
        }
    }
}

/// 开始/结束「按下一个键作为快捷键」的捕获
pub fn set_capture(on: bool) {
    if let Ok(mut g) = STATE.lock() {
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
        if hook == 0 {
            return;
        }
        // 键盘钩子用于宏录制（不影响日常输入，只在录制时记录）
        let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_proc), module, 0);
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
    if code >= 0 && crate::macros::is_recording() {
        let info = &*(l as *const KbdHookStruct);
        if info.flags & LLKHF_INJECTED == 0 {
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
        if info.flags & LLKHF_INJECTED == 0 {
            let is_down = matches!(w as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            if is_down {
                let vk = info.vk_code as u8;
                let mut triggered: Option<Vec<crate::macros::Macro>> = None;
                if let Ok(mut g) = STATE.lock() {
                    if let Some(st) = g.as_mut() {
                        if st.capture {
                            st.capture = false;
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
    if info.flags & LLMHF_INJECTED == 0 && crate::macros::is_recording() {
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

    let mut g = match STATE.lock() {
        Ok(g) => g,
        Err(_) => return CallNextHookEx(0, code, w, l),
    };
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
