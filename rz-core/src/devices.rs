//! 设备能力表（按 PID 覆盖探测不到的上限值）。

#[derive(Clone, Copy, Debug)]
pub struct DeviceSpec {
    pub family: &'static str,
    /// 该 PID 是否为无线型号/接收器（决定界面显示 2.4G 还是 USB）
    pub wireless: bool,
    pub max_dpi: u16,
    pub max_poll: u16,
    pub has_rgb: bool,
    pub has_battery: bool,
    pub stage_slots: u8,
}

impl Default for DeviceSpec {
    fn default() -> Self {
        DeviceSpec {
            family: "Razer 鼠标",
            wireless: false,
            max_dpi: 16000,
            max_poll: 1000,
            has_rgb: true,
            has_battery: true,
            stage_slots: 5,
        }
    }
}

pub fn spec_for(pid: u16) -> DeviceSpec {
    // (型号名, 最大 DPI, 最大轮询, 是否有灯, 是否有电池, 是否无线)
    let (family, max_dpi, max_poll, rgb, batt, wl) = match pid {
        0x00B6 => ("DeathAdder V3 Pro (有线)", 30000, 1000, false, true, false),
        0x00B7 => ("DeathAdder V3 Pro", 30000, 1000, false, true, true),
        0x00B9 => ("DeathAdder V3 Pro (HyperPolling)", 30000, 8000, false, true, true),
        0x00B8 => ("DeathAdder V3", 30000, 1000, false, false, false),
        0x00AA => ("Viper V2 Pro", 30000, 1000, false, true, true),
        0x0099 => ("Viper Ultimate", 20000, 1000, true, true, true),
        0x007C => ("DeathAdder V2 Pro (有线)", 20000, 1000, true, true, false),
        0x007D => ("DeathAdder V2 Pro", 20000, 1000, true, true, true),
        0x007E => ("Razer Mouse Dock", 0, 0, false, false, false),
        0x007A => ("DeathAdder V2", 20000, 1000, true, false, false),
        0x0084 => ("Basilisk V2", 20000, 1000, true, false, false),
        0x008A => ("Basilisk Ultimate", 20000, 1000, true, true, true),
        0x0037 => ("DeathAdder Chroma", 10000, 1000, true, false, false),
        0x0024 => ("DeathAdder Elite", 16000, 1000, true, false, false),
        0x0053 => ("Basilisk", 16000, 1000, true, false, false),
        0x0060 => ("Viper", 16000, 1000, true, false, false),
        _ => ("Razer 鼠标", 16000, 1000, true, true, false),
    };
    DeviceSpec {
        family,
        wireless: wl,
        max_dpi,
        max_poll,
        has_rgb: rgb,
        has_battery: batt,
        stage_slots: 5,
    }
}

/// 也暴露 91 字节控制接口、但不是鼠标的配件（例如 DeathAdder V2 Pro 的 Mouse Dock）。
pub fn is_accessory(pid: u16) -> bool {
    matches!(pid, 0x007E)
}

/// 多设备同时在线时的选择优先级：有线鼠标优先，同型号再按 PID 区分。
pub fn selection_priority(pid: u16) -> u8 {
    match pid {
        0x00B6 | 0x00B8 => 0,
        0x007C => 1,
        0x00B7 | 0x00B9 => 2,
        0x007D => 3,
        0x00AA | 0x0099 => 4,
        _ => 5,
    }
}

/// DeathAdder V2 Pro 的灯效走「单 LED（LOGO 0x04）+ 扩展亮度」协议，
/// 与项目早期为 Chroma 设备写的 0x0F/0x83 路径不同。
pub fn uses_logo_lighting(pid: u16) -> bool {
    matches!(pid, 0x007C | 0x007D)
}

/// 该型号是否支持 0x0B 校准接口（实测：DeathAdder V3 系列可用）
pub fn supports_calibration(pid: u16) -> bool {
    matches!(pid, 0x00B6 | 0x00B7 | 0x00B8 | 0x00B9)
}

/// 轮询率编码表（逆向自真机 + openrazer 协议族）
pub const POLL_CODES: &[(u8, u16)] = &[
    (0x01, 1000),
    (0x02, 500),
    (0x04, 250),
    (0x08, 125),
    (0x10, 2000),
    (0x20, 4000),
    (0x40, 8000),
];

pub fn poll_code_to_hz(code: u8) -> Option<u16> {
    POLL_CODES.iter().find(|(c, _)| *c == code).map(|(_, hz)| *hz)
}

pub fn poll_hz_to_code(hz: u16) -> Option<u8> {
    POLL_CODES.iter().find(|(_, h)| *h == hz).map(|(c, _)| *c)
}
