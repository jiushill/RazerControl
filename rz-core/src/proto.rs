//! 雷蛇控制帧协议（逆向自 Synapse 4 传输层 + 真机验证）。
//!
//! 线上格式：report id `0x00` + 90 字节负载 = 91 字节 Feature Report。
//!
//! ```text
//! 负载布局
//!  [0]      status            0x00 = 新命令
//!  [1]      transaction_id    每帧递增，应答原样带回
//!  [2..4]   remaining_packets LE16
//!  [4]      protocol_type     0xFF
//!  [5]      data_size
//!  [6]      command_class
//!  [7]      command_id
//!  [8..88]  arguments[80]
//!  [88]     crc = XOR(payload[2..88])
//!  [89]     reserved
//! ```
//!
//! 应答：status 0x01 = 忙（需轮询），0x02 = 成功，0x03 = 失败，0x05 = 不支持。

pub const REPORT_LEN: usize = 91;
pub const ARG_LEN: usize = 80;

pub const STATUS_NEW: u8 = 0x00;
pub const STATUS_BUSY: u8 = 0x01;
pub const STATUS_SUCCESS: u8 = 0x02;
pub const STATUS_FAILURE: u8 = 0x03;
pub const STATUS_TIMEOUT: u8 = 0x04;
pub const STATUS_NOT_SUPPORTED: u8 = 0x05;

/// 命令字（已验证）
#[derive(Clone, Copy, Debug)]
pub struct Cmd(pub u8, pub u8);

impl Cmd {
    pub const FIRMWARE: Cmd = Cmd(0x00, 0x81);
    pub const SERIAL: Cmd = Cmd(0x00, 0x82);
    pub const DEVICE_MODE: Cmd = Cmd(0x00, 0x84);
    /// 版本信息：args = [layoutId, editionId]（白色版 editionId = 0x80）
    pub const EDITION: Cmd = Cmd(0x00, 0x86);
    pub const POLL_RATE_GET: Cmd = Cmd(0x00, 0x85);
    pub const POLL_RATE_SET: Cmd = Cmd(0x00, 0x05);

    pub const DPI_GET: Cmd = Cmd(0x04, 0x85);
    pub const DPI_SET: Cmd = Cmd(0x04, 0x05);
    pub const DPI_STAGES_GET: Cmd = Cmd(0x04, 0x86);
    pub const DPI_STAGES_SET: Cmd = Cmd(0x04, 0x06);

    pub const BUTTON_MAP_GET: Cmd = Cmd(0x02, 0x8F);

    pub const BATTERY_GET: Cmd = Cmd(0x07, 0x80);
    pub const LOW_BATTERY_GET: Cmd = Cmd(0x07, 0x81);
    pub const LOW_BATTERY_SET: Cmd = Cmd(0x07, 0x01);
    pub const IDLE_GET: Cmd = Cmd(0x07, 0x83);
    pub const IDLE_SET: Cmd = Cmd(0x07, 0x03);
    pub const CHARGE_GET: Cmd = Cmd(0x07, 0x84);

    // 抬升/追踪（Proximity Sensor）
    // 校准（实测走 class 0x0B，不是 0x0F）
    pub const CALIB_PARAM_SET: Cmd = Cmd(0x0B, 0x0B); // [00,04,param,value]
    pub const CALIB_LIFT_SET: Cmd = Cmd(0x0B, 0x05); // [00,04,lift,land,0...] size 10
    pub const CALIB_GUARD: Cmd = Cmd(0x0B, 0x03); // 进入校准编辑 [00,04,01]

    pub const PROX_CTRL_GET: Cmd = Cmd(0x0F, 0x89);
    pub const PROX_CTRL_SET: Cmd = Cmd(0x0F, 0x09);
    pub const PROX_LIFT_GET: Cmd = Cmd(0x0F, 0x8B);
    pub const PROX_LIFT_SET: Cmd = Cmd(0x0F, 0x0B);
    pub const PROX_STATE_SET: Cmd = Cmd(0x0F, 0x03);

    pub const LED_BRIGHTNESS_GET: Cmd = Cmd(0x0F, 0x83);
    pub const LED_BRIGHTNESS_SET: Cmd = Cmd(0x0F, 0x03);
    /// 扩展亮度（DeathAdder V2 Pro 的 LOGO LED 走这条，args = [0x01, 0x04, value]）
    pub const LED_BRIGHTNESS_EXT_GET: Cmd = Cmd(0x0F, 0x84);
    pub const LED_BRIGHTNESS_EXT_SET: Cmd = Cmd(0x0F, 0x04);
    pub const LED_EFFECT_GET: Cmd = Cmd(0x0F, 0x82);
    pub const LED_EFFECT_SET: Cmd = Cmd(0x0F, 0x02);
    pub const LED_COLOR_SET: Cmd = Cmd(0x0F, 0x02);
}

/// 计算帧尾的 XOR 校验。
pub fn crc(payload: &[u8]) -> u8 {
    let mut c = 0u8;
    for b in &payload[2..88] {
        c ^= b;
    }
    c
}

/// 构建 91 字节 Feature Report。
pub fn build(tid: u8, class: u8, id: u8, args: &[u8], data_size: u8) -> [u8; REPORT_LEN] {
    let mut buf = [0u8; REPORT_LEN];
    buf[0] = 0x00; // report id
    buf[1] = STATUS_NEW;
    buf[2] = tid;
    buf[5] = 0xFF; // protocol type
    buf[6] = data_size;
    buf[7] = class;
    buf[8] = id;
    let n = args.len().min(ARG_LEN);
    buf[9..9 + n].copy_from_slice(&args[..n]);
    let c = crc(&buf[1..91]);
    buf[89] = c;
    buf[90] = 0x00;
    buf
}

/// 解析后的应答帧。
#[derive(Clone, Copy)]
pub struct Reply {
    pub status: u8,
    pub tid: u8,
    pub class: u8,
    pub id: u8,
    pub args: [u8; ARG_LEN],
}

impl Reply {
    /// 从 91 字节原始应答解析。
    pub fn parse(raw: &[u8; REPORT_LEN]) -> Reply {
        let mut args = [0u8; ARG_LEN];
        args.copy_from_slice(&raw[9..89]);
        Reply {
            status: raw[1],
            tid: raw[2],
            class: raw[7],
            id: raw[8],
            args,
        }
    }

    pub fn arg(&self, i: usize) -> u8 {
        self.args.get(i).copied().unwrap_or(0)
    }

    pub fn u16_be(&self, off: usize) -> u16 {
        ((self.arg(off) as u16) << 8) | self.arg(off + 1) as u16
    }

    pub fn is_success(&self) -> bool {
        self.status == STATUS_SUCCESS
    }

    pub fn status_name(&self) -> &'static str {
        match self.status {
            STATUS_SUCCESS => "成功",
            STATUS_BUSY => "忙",
            STATUS_FAILURE => "失败",
            STATUS_TIMEOUT => "超时",
            STATUS_NOT_SUPPORTED => "不支持",
            _ => "未知",
        }
    }
}
