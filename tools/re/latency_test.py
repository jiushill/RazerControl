# -*- coding: utf-8 -*-
"""测量 feature report 查询的真实响应延迟分布（超时放宽到 2500ms）。

用法: python latency_test.py [秒数] [间隔ms]
期间请正常使用鼠标（也可以放着不动）。
"""
import sys
import time

sys.path.insert(0, r'D:\tools\ctf\script\DS_Desktop\aday\razer-driver\tools\re')
import rzclient as Z

DURATION = float(sys.argv[1]) if len(sys.argv) > 1 else 120.0
INTERVAL = float(sys.argv[2]) if len(sys.argv) > 2 else 0.5

p, info = Z.find_control()
print('device:', info['product'], '%04X:%04X' % (info['vid'], info['pid']))
c = Z.RazerClient(p, timeout=2.5)  # 放宽到 2500ms，以测出"慢"而不是"失败"

lat = []
fails = 0
t_end = time.time() + DURATION
i = 0
while time.time() < t_end:
    t0 = time.time()
    f, e = c.xfer(0x04, 0x85, bytes(7), 7)   # DPI GET，最轻的一条
    dt = (time.time() - t0) * 1000.0
    i += 1
    if f is None or e:
        fails += 1
        print('  #%3d FAIL %s (%.0f ms)' % (i, e, dt), flush=True)
    else:
        lat.append(dt)
        if dt > 300:
            print('  #%3d 慢响应 %.0f ms' % (i, dt), flush=True)
    time.sleep(INTERVAL)

c.close()
if lat:
    lat.sort()
    def pct(q):
        return lat[min(len(lat) - 1, int(len(lat) * q))]
    print('\n=== 结果 ===')
    print('成功 %d 次, 失败 %d 次' % (len(lat), fails))
    print('min %.0f ms  p50 %.0f ms  p90 %.0f ms  p95 %.0f ms  p99 %.0f ms  max %.0f ms'
          % (lat[0], pct(0.5), pct(0.9), pct(0.95), pct(0.99), lat[-1]))
    print('>300ms: %d 次   >900ms: %d 次' % (
        sum(1 for x in lat if x > 300), sum(1 for x in lat if x > 900)))
    print('按区间:')
    buckets = [(0, 10), (10, 50), (50, 100), (100, 300), (300, 900), (900, 2500)]
    for lo, hi in buckets:
        n = sum(1 for x in lat if lo <= x < hi)
        print('  %4d-%4d ms: %d' % (lo, hi, n))
