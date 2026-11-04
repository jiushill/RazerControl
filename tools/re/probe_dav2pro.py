# -*- coding: utf-8 -*-
"""Probe the Razer DeathAdder V2 Pro (PID 0x007C wired / 0x007D wireless).

Targets the mouse's control interface explicitly (the Mouse Dock also exposes
a 91-byte control interface, so find_control() may pick the wrong device).
"""
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import razer as R
import rzclient as Z

PIDS = (0x007C, 0x007D)


def find_mouse():
    for p in R.enumerate_hid():
        i = R.info(p)
        if i and i['vid'] == 0x1532 and i['pid'] in PIDS and i['feature'] == 91:
            return p, i
    return None, None


def main():
    path, info = find_mouse()
    if not path:
        print('DeathAdder V2 Pro control interface not found')
        return 1
    print('control: %s %04X:%04X fwver=0x%04X' % (
        info['product'], info['vid'], info['pid'], info['ver']))
    c = Z.RazerClient(path, timeout=0.5)

    print('\nknown commands')
    tests = [
        ('firmware', Z.Cmd.FIRMWARE, 2),
        ('serial', Z.Cmd.SERIAL, 22),
        ('edition', (0x00, 0x86), 2),
        ('device_mode', Z.Cmd.DEVICE_MODE, 2),
        ('poll_rate', Z.Cmd.POLL_RATE_GET, 1),
        ('dpi', Z.Cmd.DPI_GET, 7),
        ('dpi_stages', Z.Cmd.DPI_STAGES_GET, 1),
        ('battery', Z.Cmd.BATTERY_GET, 2),
        ('charge', Z.Cmd.CHARGE_GET, 2),
        ('idle', Z.Cmd.IDLE_GET, 2),
        ('lowbatt', Z.Cmd.LOWBATT_GET, 1),
        ('brightness', Z.Cmd.LED_BRIGHTNESS_GET, 1),
        ('led_effect', Z.Cmd.LED_EFFECT_GET, 2),
        ('button_map', Z.Cmd.BUTTON_MAP_GET, 8),
    ]
    for name, cmd, size in tests:
        frame, err = c.get(cmd, size)
        args = frame[8:8 + min(size, 40)].hex() if frame else '-'
        print('  %-13s %-18s args=%s' % (name, err or 'ok', args))

    if '--scan' in sys.argv:
        print('\nscanning classes 0x00..0x10 x ids 0x80..0x8F')
        ok = []
        t0 = time.time()
        for cls in range(0x00, 0x11):
            for cid in range(0x80, 0x90):
                frame, err = c.xfer(cls, cid, bytes(2), 2)
                if frame and err is None:
                    ok.append((cls, cid, frame[8:16].hex()))
        print('elapsed %.1fs, supported %d' % (time.time() - t0, len(ok)))
        last = None
        for cls, cid, value in sorted(ok):
            if cls != last:
                print('--- class 0x%02X ---' % cls)
                last = cls
            print('   0x%02X/0x%02X -> %s' % (cls, cid, value))

    c.close()
    return 0


if __name__ == '__main__':
    sys.exit(main())
