//! `rz-core` — 纯 Rust 实现的雷蛇设备驱动核心。
//!
//! 直接通过 SetupAPI + hid.dll 与雷蛇设备的 HID 控制接口通信，
//! 使用逆向得到的 90 字节控制帧协议（report id 0x00 + 90B）。
//!
//! 不使用任何第三方 crate，全部 Win32 接口手写 FFI。

pub mod device;
pub mod devices;
pub mod hid;
pub mod proto;

pub use device::{pick_device, Battery, Device, DeviceError, DeviceState, DpiProfile, DpiStage};
pub use hid::{enumerate_razer, HidInfo};
pub use proto::{Cmd, Reply};
