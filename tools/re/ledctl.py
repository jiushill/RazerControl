# -*- coding: utf-8 -*-
"""Manual LED test tool for Razer mice/docks.

Examples:
    python ledctl.py list
    python ledctl.py read mouse 4
    python ledctl.py bright mouse 4 255      # mouse LOGO full brightness
    python ledctl.py bright mouse 4 0        # mouse LOGO off
    python ledctl.py read dock 5
    python ledctl.py bright dock 5 255       # dock LED full brightness
    python ledctl.py color mouse 4 ff0000    # static red on mouse LED 4
    python ledctl.py effect mouse 4 1 ff0000
    python ledctl.py off dock 5

Targets:
    mouse  -> PID 0x007C (wired) or 0x007D (wireless)
    dock   -> PID 0x007E (Mouse Dock)
    pid:XX -> explicit hex PID

Protocol:
    brightness read  : 0x0F/0x84  args [01, led, 00]
    brightness write : 0x0F/0x04  args [01, led, value]
    effect read      : 0x0F/0x82  args [01, led]
    static color     : 0x0F/0x02  args [01, led, 01, 00, 00, 01, R, G, B]
"""
import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import razer as R
import rzclient as Z

TARGETS = {
    'mouse': (0x007C, 0x007D),
    'dock': (0x007E,),
}


def find_path(target):
    pids = TARGETS.get(target)
    if pids is None:
        if target.startswith('pid:'):
            pids = (int(target.split(':', 1)[1], 16),)
        else:
            raise SystemExit('unknown target %r (use mouse/dock/pid:HEX)' % target)

    candidates = []
    for path in R.enumerate_hid():
        info = R.info(path)
        if not info or info['vid'] != 0x1532 or info['feature'] != 91:
            continue
        if info['pid'] in pids:
            candidates.append((path, info))
    if not candidates:
        raise SystemExit('no %s control interface found (feature=91)' % target)
    if target == 'mouse':
        # Prefer the interface that answers DPI GET; this avoids the dock if it
        # happens to share the PID list.
        for path, info in candidates:
            c = Z.RazerClient(path, timeout=0.4)
            frame, err = c.get(Z.Cmd.DPI_GET, 7)
            c.close()
            if frame is not None and err is None:
                return path, info
    return candidates[0]


def open_client(target):
    path, info = find_path(target)
    print('target: %s %04X:%04X %s' % (
        target, info['vid'], info['pid'], info['product']), flush=True)
    return Z.RazerClient(path, timeout=0.8)


def hex_color(s):
    v = s.strip().lstrip('#')
    if len(v) != 6:
        raise SystemExit('color must be RRGGBB')
    return bytes(int(v[i:i + 2], 16) for i in (0, 2, 4))


def cmd_read(c, args):
    led = args.led
    frame, err = c.xfer(0x0F, 0x84, bytes([0x01, led, 0x00]), 3)
    if frame is None or err:
        print('brightness: FAIL', err or 'no frame')
    else:
        raw = frame[10]
        print('brightness: raw=0x%02X (%d%%) args=%s' % (
            raw, round(raw * 100 / 255), frame[8:11].hex()))
    frame, err = c.xfer(0x0F, 0x82, bytes([0x01, led]), 2)
    if frame is None or err:
        print('effect:     FAIL', err or 'no frame')
    else:
        print('effect:     args=%s' % frame[8:8 + 8].hex())


def cmd_bright(c, args):
    value = max(0, min(255, args.value))
    frame, err = c.xfer(0x0F, 0x04, bytes([0x01, args.led, value]), 3)
    print('set brightness: %s (args=%s)' % (
        err or 'ok', bytes([0x01, args.led, value]).hex()))
    if frame is not None and not err:
        cmd_read(c, args)


def cmd_off(c, args):
    frame, err = c.xfer(0x0F, 0x04, bytes([0x01, args.led, 0x00]), 3)
    print('off: %s' % (err or 'ok'))


def cmd_effect(c, args):
    effect = args.effect
    if effect == 0x01:
        rgb = hex_color(args.color or '000000')
        payload = bytes([0x01, args.led, 0x01, 0x00, 0x00, 0x01]) + rgb
    else:
        payload = bytes([0x01, args.led, effect, 0x00, 0x00, 0x00])
    frame, err = c.xfer(0x0F, 0x02, payload, len(payload))
    print('set effect: %s (args=%s)' % (err or 'ok', payload.hex()))
    if frame is not None and not err:
        cmd_read(c, args)


def cmd_color(c, args):
    rgb = hex_color(args.color)
    payload = bytes([0x01, args.led, 0x01, 0x00, 0x00, 0x01]) + rgb
    frame, err = c.xfer(0x0F, 0x02, payload, len(payload))
    print('set color: %s (args=%s)' % (err or 'ok', payload.hex()))
    if frame is not None and not err:
        cmd_read(c, args)


def cmd_raw(c, args):
    data = bytes.fromhex(args.args.replace(' ', ''))
    frame, err = c.xfer(args.cls, args.cid, data, args.size if args.size else len(data))
    print('raw: %s args=%s' % (
        err or 'ok', frame[8:8 + min(16, len(data) + 5)].hex() if frame else '-'))


def cmd_list(_c, _args):
    for path in R.enumerate_hid():
        info = R.info(path)
        if info and info['vid'] == 0x1532 and info['feature'] == 91:
            print('%04X:%04X %-28s %s' % (
                info['vid'], info['pid'], info['product'],
                path.split('#')[-1][:48]))


def main():
    ap = argparse.ArgumentParser(description='Manual Razer LED test tool')
    sub = ap.add_subparsers(dest='cmd', required=True)

    sub.add_parser('list', help='list 91-byte control interfaces')

    p = sub.add_parser('read', help='read brightness/effect for a LED')
    p.add_argument('target')
    p.add_argument('led', type=lambda s: int(s, 0))
    p.set_defaults(func=cmd_read)

    p = sub.add_parser('bright', help='set brightness 0..255')
    p.add_argument('target')
    p.add_argument('led', type=lambda s: int(s, 0))
    p.add_argument('value', type=int)
    p.set_defaults(func=cmd_bright)

    p = sub.add_parser('off', help='set brightness 0')
    p.add_argument('target')
    p.add_argument('led', type=lambda s: int(s, 0))
    p.set_defaults(func=cmd_off)

    p = sub.add_parser('color', help='set static color RRGGBB')
    p.add_argument('target')
    p.add_argument('led', type=lambda s: int(s, 0))
    p.add_argument('color')
    p.set_defaults(func=cmd_color)

    p = sub.add_parser('effect', help='set effect id (1=static)')
    p.add_argument('target')
    p.add_argument('led', type=lambda s: int(s, 0))
    p.add_argument('effect', type=lambda s: int(s, 0))
    p.add_argument('color', nargs='?')
    p.set_defaults(func=cmd_effect)

    p = sub.add_parser('raw', help='send a raw command')
    p.add_argument('target')
    p.add_argument('cls', type=lambda s: int(s, 0))
    p.add_argument('cid', type=lambda s: int(s, 0))
    p.add_argument('args', nargs='?', default='')
    p.add_argument('size', nargs='?', type=int, default=0)
    p.set_defaults(func=cmd_raw)

    args = ap.parse_args()
    if args.cmd == 'list':
        cmd_list(None, args)
        return 0

    client = open_client(args.target)
    try:
        args.func(client, args)
    finally:
        client.close()
    return 0


if __name__ == '__main__':
    sys.exit(main())
