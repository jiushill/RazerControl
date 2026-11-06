//! 设备对象：帧收发 + 类型化的雷蛇命令封装。

use crate::devices::{poll_code_to_hz, poll_hz_to_code, spec_for, DeviceSpec};
use crate::hid::{HidDevice, HidInfo};
use crate::proto::{self, Cmd, Reply};
use std::fmt;
use std::thread::sleep;
use std::time::{Duration, Instant};
#[derive(Debug, Clone)]
pub enum DeviceError {
    Open,
    Io,
    Status(u8),
    Timeout,
    Unsupported,
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::Open => write!(f, "设备打开失败"),
            DeviceError::Io => write!(f, "HID 传输失败"),
            DeviceError::Status(s) => write!(f, "设备返回状态 0x{:02X}", s),
            DeviceError::Timeout => write!(f, "设备无应答"),
            DeviceError::Unsupported => write!(f, "设备不支持该命令"),
        }
    }
}

pub type Result<T> = std::result::Result<T, DeviceError>;

/// 枚举并选择一台可用的雷蛇鼠标。
///
/// 鼠标底座（如 DeathAdder V2 Pro 的 Mouse Dock，PID 0x007E）也会暴露
/// 91 字节控制接口，而且会对轮询率读取应答，因此必须显式跳过配件，
/// 并以 DPI 能力作为「这是鼠标」的主要判据。
pub fn pick_device() -> Option<Device> {
    let mut infos: Vec<HidInfo> = crate::hid::enumerate_razer()
        .into_iter()
        .filter(|i| i.is_control_interface())
        .filter(|i| !crate::devices::is_accessory(i.product_id))
        .filter(|i| !i.product.to_ascii_lowercase().contains("dock"))
        .collect();
    infos.sort_by_key(|i| crate::devices::selection_priority(i.product_id));

    let mut fallback: Option<Device> = None;
    for info in infos {
        if let Ok(dev) = Device::open(&info) {
            if dev.caps.dpi {
                return Some(dev);
            }
            // 无线鼠标休眠时所有能力探测都会超时；保留句柄，交给后台线程显示
            // 「休眠中」并持续探活，而不是直接判定设备不存在。
            if fallback.is_none() {
                fallback = Some(dev);
            }
        }
    }
    fallback
}

fn raw_to_percent(raw: u8) -> u8 {
    (((raw as u32) * 100 + 127) / 255).min(100) as u8
}

fn percent_to_raw(percent: u8) -> u8 {
    ((percent.min(100) as u32 * 255 + 50) / 100) as u8
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Caps {
    pub dpi: bool,
    pub dpi_stages: bool,
    pub poll_rate: bool,
    pub battery: bool,
    pub charging: bool,
    pub sleep: bool,
    pub low_battery: bool,
    pub lighting: bool,
    pub wireless: bool,
    pub calibration: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DpiStage {
    pub x: u16,
    pub y: u16,
}

#[derive(Clone, Debug, Default)]
pub struct DpiProfile {
    /// 1-based 活动段
    pub active: u8,
    pub stages: Vec<DpiStage>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
    pub present: bool,
}

/// 一次完整的设备状态快照。
#[derive(Clone, Debug, Default)]
pub struct DeviceState {
    pub name: String,
    /// 配色版本：0x00 = 黑色（默认），0x80 = 白色
    pub edition: u8,
    pub pid: u16,
    pub firmware: String,
    pub serial: String,
    pub wireless: bool,
    pub dpi: (u16, u16),
    pub profile: DpiProfile,
    pub poll_hz: u16,
    pub poll_code: u8,
    pub sleep_secs: u16,
    pub low_battery_percent: u8,
    pub battery: Battery,
    pub brightness: Option<u8>,
    /// 追踪距离档位（1=低 2=中等 3=高）与智能追踪开关
    pub liftoff: u8,
    pub tracking: u8,
    pub caps: Caps,
    pub max_dpi: u16,
    pub max_poll: u16,
    pub has_rgb: bool,
}

pub struct Device {
    hid: HidDevice,
    pub info: HidInfo,
    pub spec: DeviceSpec,
    tid: u8,
    pub caps: Caps,
}

// Synapse 和本程序共用同一块 Feature Report 缓冲区，应答可能被对方的帧覆盖。
// 用较短的超时 + 重发，比单次 900ms 干等更能适应这种竞争。
const IO_TIMEOUT: Duration = Duration::from_millis(300);
const IO_ATTEMPTS: usize = 3;
const POLL_INTERVAL: Duration = Duration::from_micros(1200);

impl Device {
    pub fn open(info: &HidInfo) -> Result<Device> {
        let hid = HidDevice::open(&info.path).ok_or(DeviceError::Open)?;
        let mut dev = Device {
            hid,
            info: info.clone(),
            spec: spec_for(info.product_id),
            tid: 0,
            caps: Caps::default(),
        };
        dev.probe();
        Ok(dev)
    }

    pub fn product_name(&self) -> &str {
        if self.info.product.is_empty() {
            self.spec.family
        } else {
            &self.info.product
        }
    }

    fn next_tid(&mut self) -> u8 {
        // 设备只接受 tid 0x01..0x7F：一旦发出 0x80 及以上的 tid，
        // 后续命令会被回 status=0x04（看起来像设备休眠，其实是我们自己越界）。
        self.tid = self.tid % 0x7F + 1;
        self.tid
    }

    /// 发送一帧并等待设备应答（自动处理 busy 轮询、stale 帧与丢帧重发）。
    pub fn xfer(&mut self, cmd: Cmd, args: &[u8], data_size: u8) -> Result<Reply> {
        let tid = self.next_tid();
        let frame = proto::build(tid, cmd.0, cmd.1, args, data_size);
        for _attempt in 0..IO_ATTEMPTS {
            if !self.hid.set_feature(&frame) {
                return Err(DeviceError::Io);
            }
            let start = Instant::now();
            let mut raw = [0u8; proto::REPORT_LEN];
            loop {
                if start.elapsed() > IO_TIMEOUT {
                    break;
                }
                if !self.hid.get_feature(&mut raw) {
                    return Err(DeviceError::Io);
                }
                let reply = Reply::parse(&raw);
                if reply.tid != tid {
                    sleep(POLL_INTERVAL);
                    continue;
                }
                match reply.status {
                    proto::STATUS_BUSY => {
                        sleep(POLL_INTERVAL);
                        continue;
                    }
                    proto::STATUS_NEW => {
                        sleep(POLL_INTERVAL);
                        continue;
                    }
                    proto::STATUS_SUCCESS => return Ok(reply),
                    proto::STATUS_NOT_SUPPORTED => return Err(DeviceError::Unsupported),
                    s => return Err(DeviceError::Status(s)),
                }
            }
        }
        Err(DeviceError::Timeout)
    }

    fn query(&mut self, cmd: Cmd, size: u8) -> Result<Reply> {
        self.xfer(cmd, &[0u8; 16], size)
    }

    fn write(&mut self, cmd: Cmd, args: &[u8]) -> Result<Reply> {
        self.xfer(cmd, args, args.len() as u8)
    }

    /// 重新探测设备能力（休眠唤醒后调用）
    pub fn probe(&mut self) {
        // 先 ping 一条：休眠时立刻返回，避免 9 条命令各等 900ms
        if self.query(Cmd::FIRMWARE, 2).is_err() {
            self.caps = Caps::default();
            return;
        }
        let mut caps = Caps::default();
        if let Ok(r) = self.query(Cmd::DPI_GET, 7) {
            caps.dpi = true;
            let _ = r;
        }
        if self.query(Cmd::DPI_STAGES_GET, 1).is_ok() {
            caps.dpi_stages = true;
        }
        if let Ok(r) = self.query(Cmd::POLL_RATE_GET, 1) {
            if poll_code_to_hz(r.arg(0)).is_some() {
                caps.poll_rate = true;
            }
        }
        if self.query(Cmd::BATTERY_GET, 2).is_ok() {
            caps.battery = true;
        }
        if self.query(Cmd::CHARGE_GET, 1).is_ok() {
            caps.charging = true;
        }
        if self.query(Cmd::IDLE_GET, 2).is_ok() {
            caps.sleep = true;
        }
        if self.query(Cmd::LOW_BATTERY_GET, 1).is_ok() {
            caps.low_battery = true;
        }
        // 灯效有两种协议：V2 Pro 用 LOGO LED + 扩展亮度，其余 Chroma 设备沿用旧路径。
        caps.lighting = if !self.spec.has_rgb {
            false
        } else if crate::devices::uses_logo_lighting(self.info.product_id) {
            self.xfer(Cmd::LED_BRIGHTNESS_EXT_GET, &[0x01, 0x04, 0x00], 3)
                .is_ok()
        } else {
            self.query(Cmd::LED_BRIGHTNESS_GET, 1).is_ok()
        };
        // 0x0F 那套在 V3 Pro 上不支持；真正的校准是 class 0x0B（按型号判定）
        caps.calibration = crate::devices::supports_calibration(self.info.product_id);
        // 连接方式以型号为准：模式字节在休眠/唤醒时会漂（0x03 ↔ 0x00），不能单独依赖
        let mode = self.query(Cmd::DEVICE_MODE, 2).map(|r| r.arg(0)).unwrap_or(0);
        let known_model = self.spec.family != "Razer 鼠标";
        caps.wireless = if known_model {
            self.spec.wireless
        } else {
            mode == 0x03
        };
        self.caps = caps;
    }

    /// 轻量探活：一条命令判断设备是否应答（休眠时只花一次超时）
    pub fn ping(&mut self) -> bool {
        self.query(Cmd::FIRMWARE, 2).is_ok()
    }

    /// 配色/版本编号：args[1]，0x00 黑色 / 0x80 白色
    pub fn edition(&mut self) -> Result<u8> {
        let r = self.query(Cmd::EDITION, 2)?;
        Ok(r.arg(1))
    }

    // ---------- 基础信息 ----------

    pub fn firmware(&mut self) -> Result<String> {
        let r = self.query(Cmd::FIRMWARE, 2)?;
        Ok(format!("v{}.{:02}", r.arg(0), r.arg(1)))
    }

    pub fn serial(&mut self) -> Result<String> {
        let r = self.query(Cmd::SERIAL, 22)?;
        let s: String = r
            .args
            .iter()
            .take(22)
            .filter(|b| **b >= 0x20 && **b < 0x7f)
            .map(|b| *b as char)
            .collect();
        Ok(s)
    }

    // ---------- DPI ----------

    pub fn dpi(&mut self) -> Result<(u16, u16)> {
        let r = self.query(Cmd::DPI_GET, 7)?;
        Ok((r.u16_be(1), r.u16_be(3)))
    }

    pub fn set_dpi(&mut self, x: u16, y: u16) -> Result<()> {
        let a = [
            0x00,
            (x >> 8) as u8,
            x as u8,
            (y >> 8) as u8,
            y as u8,
            0x00,
            0x00,
        ];
        self.write(Cmd::DPI_SET, &a).map(|_| ())
    }

    /// 读取 DPI 阶梯表：header `[flag, active, count]` + 每段 7 字节 `[n, x_hi, x_lo, y_hi, y_lo, 0, 0]`
    pub fn dpi_profile(&mut self) -> Result<DpiProfile> {
        let r = self.query(Cmd::DPI_STAGES_GET, 1)?;
        let count = r.arg(2).clamp(1, 10);
        let mut stages = Vec::new();
        for i in 0..count as usize {
            let off = 3 + i * 7;
            stages.push(DpiStage {
                x: r.u16_be(off + 1),
                y: r.u16_be(off + 3),
            });
        }
        Ok(DpiProfile {
            active: r.arg(1).max(1),
            stages,
        })
    }

    pub fn set_dpi_profile(&mut self, p: &DpiProfile) -> Result<()> {
        let count = p.stages.len().min(10);
        let mut args = Vec::with_capacity(3 + count * 7);
        args.push(0x00);
        args.push(p.active.clamp(1, count as u8));
        args.push(count as u8);
        for (i, s) in p.stages.iter().take(count).enumerate() {
            args.push((i + 1) as u8);
            args.push((s.x >> 8) as u8);
            args.push(s.x as u8);
            args.push((s.y >> 8) as u8);
            args.push(s.y as u8);
            args.push(0x00);
            args.push(0x00);
        }
        let size = args.len() as u8;
        // 少数固件要求按满长 80 字节下发，失败则回退重试
        match self.xfer(Cmd::DPI_STAGES_SET, &args, size) {
            Ok(_) => Ok(()),
            Err(DeviceError::Status(s)) if s == proto::STATUS_FAILURE || s == proto::STATUS_NOT_SUPPORTED => {
                let mut padded = args.clone();
                padded.resize(80, 0);
                self.xfer(Cmd::DPI_STAGES_SET, &padded, 82).map(|_| ())
            }
            Err(e) => Err(e),
        }
    }

    // ---------- 轮询率 ----------

    pub fn poll_hz(&mut self) -> Result<u16> {
        let r = self.query(Cmd::POLL_RATE_GET, 1)?;
        poll_code_to_hz(r.arg(0)).ok_or(DeviceError::Status(r.arg(0)))
    }

    pub fn set_poll_hz(&mut self, hz: u16) -> Result<()> {
        let code = poll_hz_to_code(hz).ok_or(DeviceError::Unsupported)?;
        self.write(Cmd::POLL_RATE_SET, &[code]).map(|_| ())
    }

    // ---------- 电源 ----------

    pub fn battery(&mut self) -> Result<Battery> {
        let r = self.query(Cmd::BATTERY_GET, 2)?;
        // 设备回报的是 0..255 的原始电量（不是百分比）：Synapse 也按 /255*100 换算
        let raw = r.arg(1);
        let percent = (((raw as u32) * 100 + 127) / 255) as u8;
        // 充电标志同样在 args[1]（args[0] 恒为 0）
        let mut charging = false;
        if let Ok(c) = self.query(Cmd::CHARGE_GET, 2) {
            charging = c.arg(1) == 0x01;
        }
        Ok(Battery {
            percent,
            charging,
            present: true,
        })
    }

    pub fn sleep_secs(&mut self) -> Result<u16> {
        let r = self.query(Cmd::IDLE_GET, 2)?;
        Ok(r.u16_be(0))
    }

    pub fn set_sleep_secs(&mut self, secs: u16) -> Result<()> {
        let a = [(secs >> 8) as u8, secs as u8];
        self.write(Cmd::IDLE_SET, &a).map(|_| ())
    }

    pub fn low_battery_percent(&mut self) -> Result<u8> {
        let r = self.query(Cmd::LOW_BATTERY_GET, 1)?;
        Ok(((r.arg(0) as u32 * 100 + 127) / 255) as u8)
    }

    pub fn set_low_battery_percent(&mut self, percent: u8) -> Result<()> {
        let raw = ((percent.clamp(1, 100) as u32 * 255 + 50) / 100) as u8;
        self.write(Cmd::LOW_BATTERY_SET, &[raw]).map(|_| ())
    }

    // ---------- 灯效（RGB 设备） ----------

    pub fn brightness(&mut self) -> Result<u8> {
        if crate::devices::uses_logo_lighting(self.info.product_id) {
            // args = [variable_storage, LOGO_LED, value]
            let r = self.xfer(Cmd::LED_BRIGHTNESS_EXT_GET, &[0x01, 0x04, 0x00], 3)?;
            Ok(raw_to_percent(r.arg(2)))
        } else if self.spec.has_rgb {
            let r = self.query(Cmd::LED_BRIGHTNESS_GET, 1)?;
            Ok(raw_to_percent(r.arg(0)))
        } else {
            Err(DeviceError::Unsupported)
        }
    }

    pub fn set_brightness(&mut self, value: u8) -> Result<()> {
        let raw = percent_to_raw(value);
        if crate::devices::uses_logo_lighting(self.info.product_id) {
            self.xfer(Cmd::LED_BRIGHTNESS_EXT_SET, &[0x01, 0x04, raw], 3)
                .map(|_| ())
        } else if self.spec.has_rgb {
            self.write(Cmd::LED_BRIGHTNESS_SET, &[raw]).map(|_| ())
        } else {
            Err(DeviceError::Unsupported)
        }
    }

    /// 静态单色（class 0x0F / id 0x02）
    pub fn set_static_color(&mut self, r: u8, g: u8, b: u8) -> Result<()> {
        if crate::devices::uses_logo_lighting(self.info.product_id) {
            // OpenRazer: extended matrix static effect (VARSTORE, LOGO_LED, STATIC, ?, ?, 0x01, RGB)
            let args = [0x01, 0x04, 0x01, 0x00, 0x00, 0x01, r, g, b];
            self.xfer(Cmd::LED_EFFECT_SET, &args, 9).map(|_| ())
        } else if self.spec.has_rgb {
            let args = [0x01, 0x01, 0x01, r, g, b, 0x00, 0x00, 0x00];
            self.write(Cmd::LED_COLOR_SET, &args).map(|_| ())
        } else {
            Err(DeviceError::Unsupported)
        }
    }

    // ---------- 校准（class 0x0B，实测） ----------

    /// 写校准参数：param 1 = 追踪距离(0..2)，param 4 = 非对称中止(0/1)
    pub fn set_calib_param(&mut self, param: u8, value: u8) -> Result<()> {
        self.xfer(Cmd::CALIB_PARAM_SET, &[0x00, 0x04, param, value], 4)
            .map(|_| ())
    }

    /// 写抬升/着陆距离（仅非对称中止开启时设备才接受）
    pub fn set_lift_land(&mut self, lift: u8, land: u8) -> Result<()> {
        let mut a = [0u8; 10];
        a[0] = 0x00;
        a[1] = 0x04;
        a[2] = lift.min(26);
        a[3] = land.min(lift.saturating_sub(1)).max(1);
        self.xfer(Cmd::CALIB_LIFT_SET, &a, 10).map(|_| ())
    }

    // ---------- 抬升 / 追踪（0x0F，多数型号不支持） ----------

    pub fn liftoff(&mut self) -> Result<u8> {
        let r = self.query(Cmd::PROX_LIFT_GET, 2)?;
        Ok(r.arg(0))
    }

    pub fn set_liftoff(&mut self, v: u8) -> Result<()> {
        self.xfer(Cmd::PROX_LIFT_SET, &[v, 0x00], 2).map(|_| ())
    }

    pub fn tracking(&mut self) -> Result<u8> {
        let r = self.query(Cmd::PROX_CTRL_GET, 2)?;
        Ok(r.arg(0))
    }

    pub fn set_tracking(&mut self, v: u8) -> Result<()> {
        self.xfer(Cmd::PROX_CTRL_SET, &[v, 0x00], 2).map(|_| ())
    }

    // ---------- 快照 ----------

    pub fn snapshot(&mut self) -> DeviceState {
        let mut st = DeviceState {
            name: self.product_name().to_string(),
            pid: self.info.product_id,
            caps: self.caps,
            max_dpi: self.spec.max_dpi,
            max_poll: self.spec.max_poll,
            has_rgb: self.spec.has_rgb,
            ..Default::default()
        };
        if let Ok(f) = self.firmware() {
            st.firmware = f;
        }
        if let Ok(e) = self.edition() {
            st.edition = e;
        }
        if let Ok(s) = self.serial() {
            st.serial = s;
        }
        st.wireless = self.caps.wireless || self.spec.wireless;
        if self.caps.dpi {
            if let Ok(d) = self.dpi() {
                st.dpi = d;
            }
        }
        if self.caps.dpi_stages {
            if let Ok(p) = self.dpi_profile() {
                st.profile = p;
            }
        }
        if self.caps.poll_rate {
            if let Ok(hz) = self.poll_hz() {
                st.poll_hz = hz;
            }
            if let Ok(r) = self.query(Cmd::POLL_RATE_GET, 1) {
                st.poll_code = r.arg(0);
            }
        }
        if self.caps.sleep {
            if let Ok(s) = self.sleep_secs() {
                st.sleep_secs = s;
            }
        }
        if self.caps.low_battery {
            if let Ok(p) = self.low_battery_percent() {
                st.low_battery_percent = p;
            }
        }
        if self.caps.battery {
            if let Ok(b) = self.battery() {
                st.battery = b;
            }
        }
        if self.caps.lighting {
            if let Ok(v) = self.brightness() {
                st.brightness = Some(v);
            }
        }
        if self.caps.calibration {
            if let Ok(v) = self.liftoff() {
                st.liftoff = v;
            }
            if let Ok(v) = self.tracking() {
                st.tracking = v;
            }
        }
        st
    }
}
