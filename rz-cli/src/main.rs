//! rzctl —— 命令行验证工具（也是 GUI 的协议参考实现）。
use rz_core::device::Device;
use rz_core::enumerate_razer;

fn pick_device() -> Option<Device> {
    rz_core::pick_device()
}

fn parse_hex(s: &str) -> Vec<u8> {
    let cleaned: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    (0..cleaned.len() / 2)
        .map(|i| u8::from_str_radix(&cleaned[i * 2..i * 2 + 2], 16).unwrap_or(0))
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let infos = enumerate_razer();
    println!("发现 {} 个雷蛇 HID 接口", infos.len());
    for i in &infos {
        println!(
            "  {:04X}:{:04X} feat={:<3} usage={:04X}/{:04X} {}",
            i.vendor_id, i.product_id, i.feature_len, i.usage_page, i.usage, i.product
        );
    }

    let mut dev = match pick_device() {
        Some(d) => d,
        None => {
            println!("未找到可控设备（需要 feature_len=91 的控制接口）");
            return;
        }
    };
    let fw = dev.firmware().unwrap_or_default();
    println!("\n已连接: {}  固件 {}", dev.product_name(), fw);
    println!("能力: {:?}", dev.caps);

    if args.is_empty() {
        let st = dev.snapshot();
        println!("\n状态快照");
        println!("  序列号    {}", st.serial);
        println!("  连接      {}", if st.wireless { "2.4G 无线" } else { "有线" });
        println!("  DPI       {} x {}", st.dpi.0, st.dpi.1);
        println!("  阶梯      活动段 {} / {} 段 {:?}", st.profile.active,
                 st.profile.stages.len(),
                 st.profile.stages.iter().map(|s| s.x).collect::<Vec<_>>());
        println!("  轮询率    {} Hz (code 0x{:02X})", st.poll_hz, st.poll_code);
        println!("  休眠      {} 秒", st.sleep_secs);
        println!("  低电阈值  {}%", st.low_battery_percent);
        println!("  电量      {}% {}", st.battery.percent,
                 if st.battery.charging { "(充电中)" } else { "" });
        return;
    }

    let cmd = args[0].as_str();
    let res: Result<String, String> = (|| {
        match cmd {
            "dpi" => {
                let x: u16 = args.get(1).and_then(|s| s.parse().ok()).ok_or("用法: dpi <x> [y]")?;
                let y: u16 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(x);
                dev.set_dpi(x, y).map_err(|e| e.to_string())?;
                Ok(format!("DPI -> {}x{}  读回 {:?}", x, y, dev.dpi()))
            }
            "poll" => {
                let hz: u16 = args.get(1).and_then(|s| s.parse().ok()).ok_or("用法: poll <hz>")?;
                dev.set_poll_hz(hz).map_err(|e| e.to_string())?;
                Ok(format!("轮询率 -> {} Hz  读回 {:?}", hz, dev.poll_hz()))
            }
            "sleep" => {
                let m: u16 = args.get(1).and_then(|s| s.parse().ok()).ok_or("用法: sleep <分钟>")?;
                dev.set_sleep_secs(m * 60).map_err(|e| e.to_string())?;
                Ok(format!("休眠 -> {} 分钟  读回 {:?}", m, dev.sleep_secs()))
            }
            "lowbatt" => {
                let p: u8 = args.get(1).and_then(|s| s.parse().ok()).ok_or("用法: lowbatt <百分比>")?;
                dev.set_low_battery_percent(p).map_err(|e| e.to_string())?;
                Ok(format!("低电阈值 -> {}%  读回 {:?}", p, dev.low_battery_percent()))
            }
            "bright" => {
                let p: u8 = args.get(1).and_then(|s| s.parse().ok()).ok_or("用法: bright <0-100>")?;
                dev.set_brightness(p).map_err(|e| e.to_string())?;
                Ok(format!("亮度 -> {}%  读回 {:?}", p, dev.brightness()))
            }
            "color" => {
                let r: u8 = args.get(1).and_then(|s| s.parse().ok()).ok_or("用法: color <r> <g> <b>")?;
                let g: u8 = args.get(2).and_then(|s| s.parse().ok()).ok_or("用法: color <r> <g> <b>")?;
                let b: u8 = args.get(3).and_then(|s| s.parse().ok()).ok_or("用法: color <r> <g> <b>")?;
                dev.set_static_color(r, g, b).map_err(|e| e.to_string())?;
                Ok(format!("静态颜色 -> #{:02X}{:02X}{:02X}", r, g, b))
            }
            "stage" => {
                let n: u8 = args.get(1).and_then(|s| s.parse().ok()).ok_or("用法: stage <段号>")?;
                let mut p = dev.dpi_profile().map_err(|e| e.to_string())?;
                p.active = n;
                dev.set_dpi_profile(&p).map_err(|e| e.to_string())?;
                let after = dev.dpi_profile().map_err(|e| e.to_string())?;
                Ok(format!("活动段 -> {}  读回 {} 当前 DPI {:?}", n, after.active, dev.dpi()))
            }
            "stagedpi" => {
                let n: usize = args.get(1).and_then(|s| s.parse().ok()).ok_or("用法: stagedpi <段号> <dpi>")?;
                let v: u16 = args.get(2).and_then(|s| s.parse().ok()).ok_or("用法: stagedpi <段号> <dpi>")?;
                let mut p = dev.dpi_profile().map_err(|e| e.to_string())?;
                if n == 0 || n > p.stages.len() {
                    return Err("段号越界".into());
                }
                p.stages[n - 1] = rz_core::DpiStage { x: v, y: v };
                dev.set_dpi_profile(&p).map_err(|e| e.to_string())?;
                let after = dev.dpi_profile().map_err(|e| e.to_string())?;
                Ok(format!("段 {} -> {}  读回 {:?}", n, v, after.stages.iter().map(|s| s.x).collect::<Vec<_>>()))
            }
            "raw" => {
                let cls = args.get(1).and_then(|s| u8::from_str_radix(s.trim_start_matches("0x"), 16).ok()).ok_or("用法: raw <cls> <id> <hexargs> <size>")?;
                let id = args.get(2).and_then(|s| u8::from_str_radix(s.trim_start_matches("0x"), 16).ok()).ok_or("缺失 id")?;
                let data = parse_hex(args.get(3).map(|s| s.as_str()).unwrap_or(""));
                let size: u8 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(data.len() as u8);
                match dev.xfer(rz_core::Cmd(cls, id), &data, size) {
                    Ok(r) => Ok(format!("status={} args={:02x?}", r.status_name(), &r.args[..size.min(40) as usize])),
                    Err(e) => Err(e.to_string()),
                }
            }
            other => Err(format!("未知命令 {}", other)),
        }
    })();

    match res {
        Ok(msg) => println!("\n{}", msg),
        Err(e) => println!("\n错误: {}", e),
    }
}
