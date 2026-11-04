# 逆向脚本（可直接跑）

全部只依赖标准库（`ctypes` 直调 `hid.dll` / `setupapi.dll`），Windows + Python 3.8+ 即可。

```powershell
python rzclient.py        # 打印设备信息 + 当前状态（自动跳过 Mouse Dock）
python scan_all.py        # 全类目 getter 扫描（class 0x00..0x10 × id 0x80..0x8F）
python sniff_long.py 200  # 被动嗅探 200 秒，抓 Synapse 自己发出的帧
python sniff_long.py 20 007D  # 指定 PID 嗅探（默认跳过 Mouse Dock）
python probe_edition.py   # 读 editionId / 硬件版本等标识
python probe_wired.py     # 插线时枚举并逐个探测所有控制接口
python probe_dav2pro.py   # DeathAdder V2 Pro：已知命令 + --scan 全类目扫描
python probe_light.py     # V2 Pro Chroma getter 矩阵（--set 会实际写灯效）
python ledgui.py          # Tkinter GUI：鼠标/底座、LED ID、亮度、颜色、效果、原始命令、闪灯定位
python ledctl.py list     # 手动灯效测试：列出发光控制接口
python ledctl.py read mouse 4    # 读鼠标 LED 0x04 亮度/效果
python ledctl.py bright mouse 4 255
python ledctl.py off mouse 4
python ledctl.py read dock 5     # 读 Mouse Dock LED 0x05
python ledctl.py bright dock 5 255
python image_bbox.py rz-app/assets/device_dav2pro.png   # 产品图 bbox，用于调 CROP
python mark_hotspots.py <原图> <输出图>                 # 画出 CROP/5 个热点，做视觉校准
```

## 多设备 / Mouse Dock

DeathAdder V2 Pro 的 Mouse Dock（`0x007E`）也会暴露 91 字节控制接口，
而且会对固件、序列号、轮询率读取应答。`rzclient.find_control()` 与
`sniff_long.py` 都会按 PID / 产品名跳过它，并优先选择能应答 DPI 的鼠标；
否则扫描到的会是底座而不是鼠标。

## 核心技巧：共享 Feature Report 缓冲区

`HidD_GetFeature` 读的是设备共享的那一块 Feature Report 缓冲区。Synapse 写完命令后，
帧还留在缓冲区里 —— 所以只申请 `GENERIC_WRITE` 权限打开句柄（读权限被厂商驱动独占），
高频读就能看到 Synapse 发出的每一条命令。不用驱动、不用注入、不用管理员。

`sniff_long.py` 就是这么抓出校准命令（`class 0x0B`）的：

```
+ 26.25s  cls=0x0B id=0x0B size=4   st=0x01 args=00040101
+ 31.52s  cls=0x0B id=0x05 size=10  st=0x01 args=00040e08000000000000
```

`st=0x01` = 请求本身（设备还没处理完），`st=0x02` = 应答。

## 注意

* 无线鼠标休眠后所有命令会超时，**探测必须重试**（脚本里都有 3 次重试和 50 ms 间隔）；
* 嗅探时先关掉本项目的 GUI，否则它自己的轮询帧会混进来。
