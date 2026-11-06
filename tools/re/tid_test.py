# -*- coding: utf-8 -*-
"""验证：失败是否由 tid 越过 0x7F 引起。

A 组：每次强制 tid=1（不递增）
B 组：正常递增（会在第 128 次跨入 0x80）
"""
import sys
import time

sys.path.insert(0, r'D:\tools\ctf\script\DS_Desktop\aday\razer-driver\tools\re')
import rzclient as Z

p, info = Z.find_control()


def run(mode, n=200):
    c = Z.RazerClient(p, timeout=1.5)
    ok = fail = 0
    first_fail = None
    tids = set()
    for i in range(1, n + 1):
        if mode == 'fixed':
            c.tid = 0            # 下一次 xfer 用 tid=1
        f, e = c.xfer(0x04, 0x85, bytes(7), 7)
        tids.add(c.tid)
        if f is None or e:
            fail += 1
            if first_fail is None:
                first_fail = i
        else:
            ok += 1
        time.sleep(0.15)
    c.close()
    print('  %-6s 成功 %3d 失败 %3d  首次失败位置=%s  tid 范围=%d..%d'
          % (mode, ok, fail, first_fail, min(tids), max(tids)))
    return ok, fail, first_fail


print('A 组：tid 固定为 1')
run('fixed')
time.sleep(1)
print('B 组：tid 正常递增')
run('inc')
