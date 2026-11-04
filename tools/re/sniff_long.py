# -*- coding: utf-8 -*-
"""长时间被动嗅探：把 Synapse 发给设备的每一帧（去重）带上时间戳打出来。"""
import os
import sys, time, ctypes
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import razer as R

DURATION = float(sys.argv[1]) if len(sys.argv) > 1 else 180.0
WANT_PID = int(sys.argv[2], 16) if len(sys.argv) > 2 else None


def pick_control():
    candidates = []
    for path in R.enumerate_hid():
        info = R.info(path)
        if not info or info.get('feature') != 91 or info.get('vid') != 0x1532:
            continue
        if WANT_PID is not None and info['pid'] != WANT_PID:
            continue
        # Mouse Dock 也暴露 91 字节接口，默认跳过，避免嗅探到错误的设备。
        if info['pid'] == 0x007E or 'dock' in info['product'].lower():
            continue
        candidates.append((path, info))
    return candidates[0] if candidates else (None, None)


p, info = pick_control()
if not p:
    sys.exit('control interface not found (usage: sniff_long.py [seconds] [pid-hex])')
h = R.k32.CreateFileW(p, 0x40000000, 3, None, 3, 0, None)
if h in (None, R.INVALID):
    sys.exit('open failed')

print('sniff %.0fs on %04X %s (%s)' % (
    DURATION, info['pid'], info['product'], p.split('#')[-1][:12]), flush=True)
seen = set()
t0 = time.time()
last_beat = 0.0
while time.time() - t0 < DURATION:
    buf = ctypes.create_string_buffer(91)
    if not R.hid.HidD_GetFeature(h, buf, 91):
        time.sleep(0.004)
        continue
    b = bytes(buf)
    if b[0] != 0x00:
        continue
    status, tid, size, cls, cid = b[1], b[2], b[6], b[7], b[8]
    args = b[9:9 + min(size, 30)]
    key = (cls, cid, size, args)
    el = time.time() - t0
    if key not in seen:
        seen.add(key)
        print('+%6.2fs  cls=0x%02X id=0x%02X size=%-2d st=0x%02X args=%s'
              % (el, cls, cid, size, status, args.hex()), flush=True)
    if el - last_beat > 15.0:
        last_beat = el
        print('... %d unique frames so far' % len(seen), flush=True)
    time.sleep(0.003)
R.close(h)
print('done: %d unique frames' % len(seen), flush=True)
