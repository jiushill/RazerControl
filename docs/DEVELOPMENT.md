# 二次开发指南

这份文档写给想改这个驱动的人。所有结论都来自真机逆向 + 实测，不是猜的。

## 1. 目录结构

```
razer-driver/
├─ rz-core/                 # 驱动核心：零第三方依赖，手写 Win32 FFI
│  ├─ src/hid.rs            # SetupAPI 枚举 + hid.dll Feature Report 收发
│  ├─ src/proto.rs          # 91 字节帧的构造/解析、XOR 校验、命令字表
│  ├─ src/device.rs         # 类型化命令：DPI/阶梯/轮询/电量/休眠/阈值/校准/灯效
│  └─ src/devices.rs        # PID → 能力表（最大 DPI、轮询上限、是否有灯/电池/无线）
├─ rz-cli/                  # rzctl：命令行工具，也是最好的协议调试器
├─ rz-app/                  # RazerControl：egui 原生界面
│  ├─ src/theme.rs          # 设计系统：配色、字体、卡片、柔和投影
│  ├─ src/widgets.rs        # 全自绘控件：滑杆、开关、分段选择、环形电量、图标
│  ├─ src/photo.rs          # 官方产品图：白底抠图 → 纹理 → 热点
│  ├─ src/art.rs            # 矢量鼠标插画（无素材型号的回退）
│  ├─ src/pages/mod.rs      # 六个页面
│  ├─ src/backend.rs        # 设备工作线程（所有 HID I/O 不阻塞 UI）
│  ├─ src/tray.rs           # 系统托盘（Shell_NotifyIconW + 自绘菜单）
│  ├─ src/hook.rs           # WH_MOUSE_LL 软件按键映射
│  └─ assets/               # 官方 ICO + 产品图（编译期嵌入）
└─ tools/
   ├─ click.ps1 / shot2.ps1 # 自动化点击/截图（写文档用）
   └─ re/                   # 逆向脚本（Python，可直接跑）
```

数据流：`UI(pages) ──Req──> backend 工作线程 ──> rz-core::Device ──> hid.dll ──> 设备`
反过来 `设备 ──> Device::snapshot() ──Evt::State──> UI`。UI 线程永远不直接碰 HID。

## 2. 加一条新命令（4 步）

以「设置休眠时间」为例，每一步都能编译通过：

**① `rz-core/src/proto.rs`** —— 登记命令字

```rust
pub const IDLE_GET: Cmd = Cmd(0x07, 0x83);
pub const IDLE_SET: Cmd = Cmd(0x07, 0x03);
```

命名约定（Razer 自己的）：**读 = `0x8X`，写 = `0x0X`，同一功能的读写只差最高位**。
先在真机上用 `rzctl raw` 确认设备真的应答，再往下写。

**② `rz-core/src/device.rs`** —— 包成类型化方法

```rust
pub fn sleep_secs(&mut self) -> Result<u16> {
    let r = self.query(Cmd::IDLE_GET, 2)?;   // size = data_size
    Ok(r.u16_be(0))                          // 大端
}
pub fn set_sleep_secs(&mut self, secs: u16) -> Result<()> {
    self.write(Cmd::IDLE_SET, &[(secs >> 8) as u8, secs as u8]).map(|_| ())
}
```

**③ `rz-app/src/backend.rs`** —— 加 `Req` 变体 + 分支

```rust
pub enum Req { ..., SetSleepMin(u16) }

Req::SetSleepMin(min) => match d.set_sleep_secs(min * 60) {
    Ok(_) => ok(tx, &format!("休眠时间已设为 {} 分钟", min)),
    Err(e) => fail(tx, format!("设置休眠失败: {}", err_text(&e))),
},
```

**④ `rz-app/src/pages/*.rs`** —— 界面里发请求

```rust
if widgets::slider(ui, egui::Id::new("sleep"), &mut v, 1.0, 15.0, 1.0, w, ACCENT)
    .drag_stopped()
{
    app.backend.req(Req::SetSleepMin(v as u16));
}
```

## 3. 协议调试

### 3.1 直接发一帧

```powershell
rzctl                    # 枚举接口 + 完整状态快照
rzctl raw 07 80 0000 2   # class 0x07 id 0x80，两个 0 参数，data_size=2
rzctl dpi 1600
rzctl stage 3
```

`rzctl raw` 会打印 `status=成功 args=[...]`。`status` 含义：
`0x01` 忙（重试）、`0x02` 成功、`0x03` 失败/不支持、`0x04` 超时、`0x05` 不支持。

### 3.2 抓 Synapse 自己的帧（关键技巧）

`HidD_GetFeature` 读的是**设备共享的 Feature Report 缓冲区**，Synapse 写完命令后，
缓冲区里就留着它刚发的帧。所以只要开着 Synapse，用写权限句柄高频读，就能看到它的全部命令 ——
不用注入、不用驱动、不用管理员。

```powershell
# 让 Synapse 停在要观察的页面，然后：
python tools/re/sniff_long.py 200      # 200 秒被动嗅探
```

输出形如（这就是当初抓出校准命令的原始记录）：

```
+ 26.25s  cls=0x0B id=0x0B size=4   st=0x01 args=00040101
+ 31.52s  cls=0x0B id=0x05 size=10  st=0x01 args=00040e08000000000000
```

`st=0x01` 是请求本身（还没被设备处理），正好用来拿"发出去的字节"。

### 3.3 扫全类目

```powershell
python tools/re/scan_all.py     # class 0x00..0x10 × id 0x80..0x8F 全扫，打印所有有应答的 getter
```

注意：**必须重试**。无线设备休眠后第一条命令往往超时，直接判死不准确。

## 4. 支持新设备

1. **能力表** `rz-core/src/devices.rs`：填 `(型号名, 最大DPI, 最大轮询, 有灯?, 有电池?, 无线?)`。
   最大 DPI / 轮询率无法从设备读出来，只能按型号写死。
   同时维护 `is_accessory()` / `selection_priority()`：像 DeathAdder V2 Pro 的 Mouse Dock（`0x007E`）
   也会暴露 91 字节控制接口，必须跳过，不能把底座当成鼠标。
2. **产品图** `rz-app/src/photo.rs`：
   - 素材放 `rz-app/assets/device_<型号>.png`（建议官方顶视渲染）；
   - `has_photo_for(pid)` 里加 PID；
   - `CROP_DAV3PRO` / `CROP_DAV2PRO` 与对应的 `HOTSPOTS_*` 是整张图的归一化裁剪框和
     5 个按键中心点，换图要单独调；
   - 配色用设备上报的 `editionId`（`0x00/0x86` 的 `args[1]`）选：`0x00` 黑、`0x80` 白；
     新配色先 `rzctl raw 00 86 0000 2` 读出它的 `editionId`，再加一个分支和对应素材。
3. **灯效协议**：V2 Pro 走「单 LOGO LED + 扩展亮度」：
   亮度 `0x0F/0x84` 读、`0x0F/0x04` 写，args `[0x01, 0x04, value]`；
   静态色 `0x0F/0x02`，args `[0x01, 0x04, 0x01, 0x00, 0x00, 0x01, R, G, B]`。
   新设备先用 `rzctl raw 0F 84 0104 3` 验证，再决定是否复用 `uses_logo_lighting()`。
4. **校准命令**：目前只对 DeathAdder V3 系列（`0x00B6~0x00B9`）验证过 `class 0x0B`，
   其他型号需要在 `supports_calibration()` 里显式放开，并自行验证 payload。

## 5. 踩过的坑（照这个顺序排查，能省一天）

| # | 现象 | 原因 | 解法 |
| --- | --- | --- | --- |
| 1 | `CreateFile` 读权限被拒（err 5） | `RzDev_*.sys` 独占读 | **只申请 `GENERIC_WRITE`**，`HidD_GetFeature` 在写句柄上依然可用 |
| 2 | 第一帧总是失败/无应答 | 设备回 `status=0x01`（忙），应答滞后 | 轮询 + 用 `transaction_id` 过滤别人的应答 |
| 3 | 电量显示偏大 3 倍 | `args[1]` 是 0..255 原始值 | `raw * 100 / 255`（Synapse 同此换算） |
| 4 | 充电状态永远是"放电" | 充电标志也在 `args[1]`，`args[0]` 恒为 0 | 读 `args[1] == 0x01` |
| 5 | 有线/无线来回跳 | `0x00/0x84` 的模式字节会漂（`0x03`/`0x00`） | 以 **PID** 判断连接方式，模式字节只作参考 |
| 6 | 睡一觉后彻底失联 | 休眠/重枚举让 HID 句柄失效 | 连续 5 次无应答就丢弃句柄、重新枚举并 probe |
| 7 | 抬升/着陆写不进去（`0x03`） | 设备要求先开「非对称中止」 | 先写 `0x0B/0x0B [00,04,04,01]`，再写 `0x0B/0x05` |
| 8 | 读回校准值全是错的 | `0x0B/0x85` 只是**回显请求**，不是 getter | 值自己存（Synapse 也存本地） |
| 9 | 白色机身被抠出洞 | 白底白鼠，阈值把机身吃掉了 | 背景判定收紧到 `亮度>=248 && RGB极差<=6` |
| 10 | 任务栏图标发虚 | winit 只接受一张位图，Windows 再降采样 | 按系统 DPI 选原生尺寸（100% → 32×32），并把 ICO 组作为 exe 资源嵌入 |
| 11 | 无边框窗口 `ViewportCommand::Visible(false)` 无效 | winit/egui 在无边框窗口上的已知行为 | 直接 `ShowWindow(SW_HIDE/SW_SHOW)` |
| 12 | 进程看似还在但界面不刷新 | eframe 在窗口隐藏后不再收到重绘 | 托盘动作里 `ctx.request_repaint()`，并且 HID 全在后台线程 |
| 13 | 鼠标 DPI 键切换后界面慢 | Synapse 用鼠标的 `hardwareEventReportID`（report 5）主动事件；本程序只读 Feature Report | 清醒时 250ms 轮询 DPI，DPI 变化时补读阶梯表 |
| 14 | 鼠标没休眠却显示休眠 | Synapse 抢占共享 Feature 缓冲区，单次应答丢失被当成无应答 | `xfer` 300ms × 3 重发；连续 2 次读不到 DPI/阶梯表才标休眠 |

## 6. 构建

```powershell
# 需要 Rust（任意近期 stable）+ MinGW 或 MSVC 工具链
cargo build --release            # 全部工作区
cargo build --release -p rz-app  # 只出 GUI
cargo build --release -p rz-cli  # 只出 rzctl

# GUI 的图标资源用 windres 编译（build.rs 自动找，找不到会跳过并给 warning）
```

国内网络如果卡在 `Updating crates.io index`，在项目根建 `.cargo/config.toml`：

```toml
[source.crates-io]
replace-with = "rsproxy-sparse"
[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"
```

## 7. Roadmap / 想做的方向

- [ ] **灯效**：`0x0F` 那套在这台机器上不支持，需要支持 Chroma 的设备来验证（代码路径已留）
- [ ] **硬件按键重映射**：`0x02/0x8F` 返回 `0x05`，这代只支持软件层映射；新一代设备可尝试 `0x08/0x84`
- [ ] **板载配置文件切换**：`0x00` 组里有 profile 相关命令，未系统验证
- [ ] **BLE（protocol 3.0/4.0）**：仅实现了 USB/HID 的 90+1 字节路径，蓝牙通道未接
- [ ] **多设备同时管理**：`pick_device()` 已经会枚举所有控制接口，但 UI 只展示一个
- [ ] **电压曲线**：`0x07/0x85` 疑似电池 mV，可用它拟合更准的百分比

## 8. 逆向脚本说明（`tools/re/`）

| 脚本 | 作用 |
| --- | --- |
| `razer.py` | HID 枚举/打开/Feature Report 收发（`ctypes` 直接调 hid.dll） |
| `rzclient.py` | 协议客户端：帧构造 + busy 轮询 + tid 过滤 |
| `sniff_long.py` | 被动嗅探共享缓冲区，抓 Synapse 发出的帧 |
| `scan_all.py` | 全类目 getter 扫描 |
| `probe_edition.py` | 读 `editionId` / 硬件版本等标识 |
| `probe_wired.py` | 多控制接口逐个探测（插线时会出现两个接口） |

这几个脚本不依赖任何第三方库（只用 `ctypes`），Windows + Python 3.8 即可跑。

## 9. 宏引擎（软件层）

`rz-app/src/macros.rs` 是自包含的：事件模型 + 录制状态 + 回放 + 文本存储。

**为什么不走设备**：DeathAdder V3 Pro 的 `class 0x08` 全部无应答、`0x00/0x00` 能力位图也不支持，
说明没有硬件宏存储。Synapse 对这类设备同样在 PC 侧执行宏。

**接进钩子的方式**（`hook.rs`）：

1. 装两个低级钩子：`WH_MOUSE_LL`（已有）+ `WH_KEYBOARD_LL`（新增，仅录制时记录）；
2. 在 remap 逻辑**之前**记录，且只在 `!(flags & LLMHF_INJECTED)` 时记 —— 否则回放会被录进去；
3. 鼠标移动记录的是**增量**（低级钩子给的是绝对坐标，要自己减上一次）。

**加一种新事件类型**：在 `macros.rs` 里加常量 → `push()` 调用点 → `play()` 的 match 分支，
文本存储和 UI 都不用改。

**UI 结构**（`pages::macros`）：状态卡（录制 + 回放速度）→ 宏列表 → 展开后的事件表。
事件表的延迟是 `egui::DragValue`，改动后立刻写回 `macros.txt` 并调用 `hook::set_macros()`，
让钩子里的宏快照保持同步 —— `Action::Macro(i)` 触发时用的是这份快照。

注意：页面外层已经有 `ScrollArea`，**不要再套内层 ScrollArea**（嵌套会被外层裁成很矮一条）。

**绑定**：`Action::Macro(u8)`，编码 `20 + i`，上限 8 个；下拉菜单里宏排在「保持默认/禁用」之后、
其它动作之前 —— 因为菜单经常超出窗口底部，放后面就点不到了。
