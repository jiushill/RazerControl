# -*- coding: utf-8 -*-
"""Razer control-protocol reference client (validated on DeathAdder V3 Pro, PID 0x00B7)."""
import ctypes
import ctypes.wintypes as wt
import time
import razer as R

STATUS_NEW = 0x00
STATUS_BUSY = 0x01
STATUS_SUCCESS = 0x02
STATUS_FAILURE = 0x03
STATUS_TIMEOUT = 0x04
STATUS_UNSUPPORTED = 0x05


class Cmd(object):
    FIRMWARE = (0x00, 0x81)
    SERIAL = (0x00, 0x82)
    DEVICE_MODE = (0x00, 0x84)
    POLL_RATE_GET = (0x00, 0x85)
    POLL_RATE_SET = (0x00, 0x05)
    DPI_GET = (0x04, 0x85)
    DPI_SET = (0x04, 0x05)
    DPI_STAGES_GET = (0x04, 0x86)
    BUTTON_MAP_GET = (0x02, 0x8F)
    BATTERY_GET = (0x07, 0x80)
    CHARGE_GET = (0x07, 0x84)
    IDLE_GET = (0x07, 0x83)
    IDLE_SET = (0x07, 0x03)
    LOWBATT_GET = (0x07, 0x81)
    LOWBATT_SET = (0x07, 0x01)
    LED_BRIGHTNESS_GET = (0x0F, 0x83)
    LED_BRIGHTNESS_SET = (0x0F, 0x03)
    LED_BRIGHTNESS_EXT_GET = (0x0F, 0x84)
    LED_BRIGHTNESS_EXT_SET = (0x0F, 0x04)
    LED_EFFECT_GET = (0x0F, 0x82)
    LED_EFFECT_SET = (0x0F, 0x02)


class RazerClient(object):
    def __init__(self, path, report_len=91, timeout=1.0, verbose=False):
        self.h = R.k32.CreateFileW(path, 0x40000000, 3, None, 3, 0, None)
        if self.h in (None, R.INVALID):
            raise OSError('open failed err=%d' % ctypes.get_last_error())
        self.report_len = report_len
        self.tid = 0
        self.timeout = timeout
        self.verbose = verbose

    def close(self):
        R.close(self.h)

    def _build(self, cls, cid, args, size=None):
        pay = bytearray(90)
        pay[0] = STATUS_NEW
        # 设备只接受 0x01..0x7F 的 tid
        self.tid = (self.tid % 0x7F) + 1
        pay[1] = self.tid
        pay[4] = 0xFF
        pay[5] = len(args) if size is None else size
        pay[6] = cls
        pay[7] = cid
        pay[8:8 + len(args)] = args
        pay[88] = R.crc(pay)
        return bytes([0x00]) + bytes(pay)

    def _read(self):
        buf = ctypes.create_string_buffer(self.report_len)
        if not R.hid.HidD_GetFeature(self.h, buf, self.report_len):
            return None
        return bytes(buf)

    def xfer(self, cls, cid, args=b'', size=None):
        req = self._build(cls, cid, args, size)
        if self.verbose:
            print('  >', req.hex())
        b = ctypes.create_string_buffer(req, len(req))
        if not R.hid.HidD_SetFeature(self.h, b, len(req)):
            return None, 'set_feature err=%d' % ctypes.get_last_error()
        tid = req[2]
        deadline = time.time() + self.timeout
        while time.time() < deadline:
            raw = self._read()
            if raw is None:
                return None, 'get_feature err=%d' % ctypes.get_last_error()
            frame = raw[1:] if raw[0] == 0x00 else raw
            if len(frame) < 90:
                return None, 'short frame'
            if frame[1] != tid:
                time.sleep(0.002)
                continue
            st = frame[0]
            if st == STATUS_BUSY:
                time.sleep(0.002)
                continue
            if self.verbose:
                print('  <', raw.hex())
            return frame, None if st == STATUS_SUCCESS else 'status=0x%02x' % st
        return None, 'timeout'

    def get(self, cmd, size, fill=0x00):
        return self.xfer(cmd[0], cmd[1], bytes([fill]) * size, size)

    def set(self, cmd, args):
        return self.xfer(cmd[0], cmd[1], args, len(args))


def find_control():
    """Pick the mouse control interface, not the Mouse Dock (PID 0x007E).

    Both expose a 91-byte feature report; the dock also answers some basic
    getters, so prefer a device that actually answers DPI GET.
    """
    candidates = []
    for p in R.enumerate_hid():
        i = R.info(p)
        if not i or i['vid'] != 0x1532 or i['feature'] != 91:
            continue
        if i['pid'] == 0x007E or 'dock' in i['product'].lower():
            continue
        candidates.append((p, i))
    for p, i in candidates:
        c = RazerClient(p, timeout=0.4)
        frame, err = c.get(Cmd.DPI_GET, 7)
        c.close()
        if frame is not None and err is None:
            return p, i
    if candidates:
        return candidates[0]
    return None, None


if __name__ == '__main__':
    path, info = find_control()
    print('control:', info['product'], '%04X:%04X' % (info['vid'], info['pid']), 'fwver=0x%04X' % info['ver'])
    c = RazerClient(path, verbose='-v' in __import__('sys').argv)
    v2pro = info['pid'] in (0x007C, 0x007D)
    for name, cmd, size in [
            ('firmware', Cmd.FIRMWARE, 2),
            ('serial', Cmd.SERIAL, 22),
            ('device_mode', Cmd.DEVICE_MODE, 1),
            ('poll_rate', Cmd.POLL_RATE_GET, 1),
            ('dpi', Cmd.DPI_GET, 7),
            ('dpi_stages', Cmd.DPI_STAGES_GET, 1),
            ('battery', Cmd.BATTERY_GET, 2),
            ('charge_status', Cmd.CHARGE_GET, 1),
            ('idle_time', Cmd.IDLE_GET, 2),
            ('low_batt', Cmd.LOWBATT_GET, 1),
            ('brightness', Cmd.LED_BRIGHTNESS_GET, 1),
            ('led_effect', Cmd.LED_EFFECT_GET, 2)]:
        if v2pro and name == 'brightness':
            frame, err = c.xfer(0x0F, 0x84, bytes([0x01, 0x04, 0x00]), 3)
            shown = 3
        elif v2pro and name == 'led_effect':
            frame, err = c.xfer(0x0F, 0x82, bytes([0x01, 0x04]), 2)
            shown = 2
        else:
            frame, err = c.get(cmd, size)
            shown = size
        if frame is None:
            print('%-13s FAIL %s' % (name, err))
        else:
            print('%-13s %-12s args=%s' % (name, err or 'ok', frame[8:8 + shown].hex()))
    c.close()
