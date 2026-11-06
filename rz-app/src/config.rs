//! 配置文件持久化（%APPDATA%\RazerControl\config.json）

use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Pass,
    Disabled,
    Left,
    Right,
    Middle,
    Back,
    Forward,
    DpiCycle,
    Copy,
    Paste,
    PlayPause,
    VolUp,
    VolDown,
    /// 触发第 n 个宏
    Macro(u8),
}

impl Action {
    /// 可绑定的宏数量上限
    pub const MAX_MACROS: u8 = 8;

    pub const ALL: [Action; 12] = [
        Action::Pass,
        Action::Disabled,
        Action::Left,
        Action::Right,
        Action::Middle,
        Action::Back,
        Action::Forward,
        Action::DpiCycle,
        Action::Copy,
        Action::Paste,
        Action::PlayPause,
        Action::VolUp,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Action::Pass => "保持默认",
            Action::Disabled => "禁用按键",
            Action::Left => "鼠标左键",
            Action::Right => "鼠标右键",
            Action::Middle => "鼠标中键",
            Action::Back => "后退",
            Action::Forward => "前进",
            Action::DpiCycle => "切换 DPI 段",
            Action::Copy => "复制 (Ctrl+C)",
            Action::Paste => "粘贴 (Ctrl+V)",
            Action::PlayPause => "播放/暂停",
            Action::VolUp => "音量 +",
            Action::VolDown => "音量 −",
            Action::Macro(_) => "宏",
        }
    }

    pub fn code(&self) -> u8 {
        match self {
            Action::Pass => 0,
            Action::Disabled => 1,
            Action::Left => 2,
            Action::Right => 3,
            Action::Middle => 4,
            Action::Back => 5,
            Action::Forward => 6,
            Action::DpiCycle => 7,
            Action::Copy => 8,
            Action::Paste => 9,
            Action::PlayPause => 10,
            Action::VolUp => 11,
            Action::VolDown => 12,
            Action::Macro(n) => 20 + n,
        }
    }

    pub fn from_code(c: u8) -> Action {
        if (20..20 + Action::MAX_MACROS).contains(&c) {
            return Action::Macro(c - 20);
        }
        Action::ALL
            .iter()
            .copied()
            .find(|a| a.code() == c)
            .unwrap_or(Action::Pass)
    }
}

#[derive(Clone)]
pub struct Config {
    pub last_page: usize,
    pub sync_xy: bool,
    pub remap_enabled: bool,
    /// 索引: 0=左 1=右 2=中 3=后退 4=前进
    pub remaps: [Action; 5],
    pub launch_on_start: bool,
    /// 校准：0=低 1=中等 2=高
    pub calib_tracking: u8,
    pub calib_asym: bool,
    pub calib_lift: u8,
    pub calib_land: u8,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            last_page: 0,
            sync_xy: true,
            remap_enabled: false,
            remaps: [
                Action::Pass,
                Action::Pass,
                Action::Pass,
                Action::Pass,
                Action::Pass,
            ],
            launch_on_start: false,
            calib_tracking: 1,
            calib_asym: false,
            calib_lift: 15,
            calib_land: 8,
        }
    }
}

fn dir() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("RazerControl")
}

pub fn path() -> PathBuf {
    dir().join("config.json")
}

impl Config {
    pub fn load() -> Config {
        let mut cfg = Config::default();
        if let Ok(text) = fs::read_to_string(path()) {
            if let Some(v) = crate::json::parse(&text) {
                use crate::json::{get_bool, get_num};
                cfg.last_page = get_num(&v, "last_page").unwrap_or(0.0) as usize;
                cfg.sync_xy = get_bool(&v, "sync_xy").unwrap_or(true);
                cfg.remap_enabled = get_bool(&v, "remap_enabled").unwrap_or(false);
                cfg.launch_on_start = get_bool(&v, "launch_on_start").unwrap_or(false);
                cfg.calib_tracking = get_num(&v, "calib_tracking").unwrap_or(1.0) as u8;
                cfg.calib_asym = get_bool(&v, "calib_asym").unwrap_or(false);
                cfg.calib_lift = get_num(&v, "calib_lift").unwrap_or(15.0) as u8;
                cfg.calib_land = get_num(&v, "calib_land").unwrap_or(8.0) as u8;
                for i in 0..5 {
                    if let Some(code) = get_num(&v, &format!("btn{}", i)) {
                        cfg.remaps[i] = Action::from_code(code as u8);
                    }
                }
            }
        }
        cfg
    }

    pub fn save(&self) -> std::io::Result<()> {
        fs::create_dir_all(dir())?;
        let mut s = String::from("{\n");
        s.push_str(&format!("  \"last_page\": {},\n", self.last_page));
        s.push_str(&format!("  \"sync_xy\": {},\n", self.sync_xy));
        s.push_str(&format!("  \"remap_enabled\": {},\n", self.remap_enabled));
        s.push_str(&format!("  \"launch_on_start\": {},\n", self.launch_on_start));
        s.push_str(&format!("  \"calib_tracking\": {},\n", self.calib_tracking));
        s.push_str(&format!("  \"calib_asym\": {},\n", self.calib_asym));
        s.push_str(&format!("  \"calib_lift\": {},\n", self.calib_lift));
        s.push_str(&format!("  \"calib_land\": {},\n", self.calib_land));
        for i in 0..5 {
            s.push_str(&format!("  \"btn{}\": {},\n", i, self.remaps[i].code()));
        }
        s.push_str("}\n");
        fs::write(path(), s)
    }
}
