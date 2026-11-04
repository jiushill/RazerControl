# -*- coding: utf-8 -*-
"""Probe the DeathAdder V2 Pro Chroma interface (class 0x0F)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import razer as R
import rzclient as Z

PIDS = (0x007C, 0x007D)


def wanted_pids():
    for arg in sys.argv[1:]:
        if arg.startswith('--pid='):
            return (int(arg.split('=', 1)[1], 16),)
    return PIDS


def find_mouse():
    pids = wanted_pids()
    for p in R.enumerate_hid():
        i = R.info(p)
        if i and i['vid'] == 0x1532 and i['pid'] in pids and i['feature'] == 91:
            return p
    return None


def main():
    path = find_mouse()
    if not path:
        print('mouse control interface not found')
        return 1
    c = Z.RazerClient(path, timeout=0.5)

    print('getter matrix')
    for cid in (0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88):
        for size in (1, 2, 3, 4, 5, 6, 8, 10):
            frame, err = c.xfer(0x0F, cid, bytes(size), size)
            if frame and err is None:
                print('  0F/%02X size=%-2d args=%s' % (cid, size, frame[8:8 + size].hex()))

    if '--set' in sys.argv:
        # set LED effect / color probes; every write is reported back with status only
        print('\nsetter probes')
        payloads = [
            (0x02, bytes([0x01, 0x01, 0x01])),
            (0x02, bytes([0x01, 0x00, 0x01, 0xFF, 0x00, 0x00])),
            (0x04, bytes([0x01, 0x01, 0x50])),
            (0x04, bytes([0x00, 0x01, 0x50])),
            (0x01, bytes([0x01, 0x01, 0x44, 0xD6, 0x2C])),
            (0x01, bytes([0x00, 0x01, 0x44, 0xD6, 0x2C])),
            (0x00, bytes([0x01, 0x01, 0x01])),
        ]
        for cid, args in payloads:
            frame, err = c.xfer(0x0F, cid, args, len(args))
            print('  0F/%02X %-20s -> %s %s' % (
                cid, args.hex(), err or 'ok',
                frame[8:8 + len(args)].hex() if frame else '-'))

    c.close()
    return 0


if __name__ == '__main__':
    sys.exit(main())
