# -*- coding: utf-8 -*-
"""Razer LED Lab — manual GUI for testing mouse/dock lighting.

Run:
    python tools/re/ledgui.py

It reuses razer.py / rzclient.py and needs no third-party packages.
"""
import os
import sys
import tkinter as tk
from tkinter import ttk

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import razer as R
import rzclient as Z

BG = '#0f1512'
CARD = '#161d1a'
CARD2 = '#1b2420'
FG = '#e8f0ea'
MUTED = '#8b9a90'
ACCENT = '#44d62c'
BORDER = '#2a352f'
WARN = '#ffb02e'
DANGER = '#ff5d5d'


def parse_hex(value, default=0):
    try:
        text = str(value).strip().lower().replace('0x', '')
        return int(text, 16) if text else default
    except ValueError:
        return default


def hs(value):
    return ' '.join('%02X' % b for b in value)


class LedLab(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title('Razer LED Lab — DeathAdder V2 Pro')
        self.geometry('980x760')
        self.minsize(900, 680)
        self.configure(bg=BG)
        self.client = None
        self.info = None
        self.path = None
        self.flash_clients = []
        self._style()
        self._build()
        self.after(150, self.connect)

    # ---------------- UI ----------------
    def _style(self):
        style = ttk.Style(self)
        try:
            style.theme_use('clam')
        except tk.TclError:
            pass
        style.configure('.', background=BG, foreground=FG, fieldbackground=CARD,
                        bordercolor=BORDER, lightcolor=CARD, darkcolor=CARD)
        style.configure('TLabel', background=BG, foreground=FG)
        style.configure('Muted.TLabel', foreground=MUTED)
        style.configure('Accent.TLabel', foreground=ACCENT)
        style.configure('Warn.TLabel', foreground=WARN)
        style.configure('Danger.TLabel', foreground=DANGER)
        style.configure('TLabelframe', background=BG, bordercolor=BORDER)
        style.configure('TLabelframe.Label', background=BG, foreground=ACCENT)
        style.configure('TButton', background=CARD, foreground=FG,
                        bordercolor=BORDER, focuscolor=ACCENT, padding=(8, 5))
        style.map('TButton',
                  background=[('active', CARD2), ('pressed', '#26352e')],
                  foreground=[('disabled', MUTED)])
        style.configure('Accent.TButton', background=ACCENT, foreground='#081008',
                        bordercolor=ACCENT, padding=(8, 5))
        style.map('Accent.TButton', background=[('active', '#5be842')])
        style.configure('TEntry', fieldbackground=CARD, foreground=FG,
                        insertcolor=FG, bordercolor=BORDER)
        style.configure('TCombobox', fieldbackground=CARD, foreground=FG,
                        background=CARD, arrowcolor=FG, bordercolor=BORDER)
        style.map('TCombobox', fieldbackground=[('readonly', CARD)])
        style.configure('TScale', background=BG, troughcolor='#2a352f',
                        bordercolor=BORDER, lightcolor=ACCENT, darkcolor=ACCENT)
        style.configure('Horizontal.TScale', background=BG, troughcolor='#2a352f')

    def _build(self):
        pad = {'padx': 10, 'pady': 6}
        outer = ttk.Frame(self, padding=12)
        outer.pack(fill='both', expand=True)

        # top: target + connect
        top = ttk.LabelFrame(outer, text='目标设备', padding=10)
        top.pack(fill='x')
        ttk.Label(top, text='目标').grid(row=0, column=0, sticky='w')
        self.target = tk.StringVar(value='鼠标 (007D/007C)')
        self.target_box = ttk.Combobox(
            top, textvariable=self.target, state='readonly', width=24,
            values=['鼠标 (007D/007C)', '充电底座 (007E)', '自定义 PID'])
        self.target_box.grid(row=0, column=1, sticky='w', padx=(6, 12))
        ttk.Label(top, text='PID').grid(row=0, column=2, sticky='w')
        self.pid_var = tk.StringVar(value='007D')
        ttk.Entry(top, textvariable=self.pid_var, width=8).grid(
            row=0, column=3, sticky='w', padx=(6, 12))
        ttk.Button(top, text='连接 / 刷新', style='Accent.TButton',
                   command=self.connect).grid(row=0, column=4, padx=(0, 8))
        ttk.Button(top, text='读取两个灯区', command=self.read_both).grid(
            row=0, column=5, padx=(0, 8))
        ttk.Button(top, text='清空日志', command=self.clear_log).grid(
            row=0, column=6)
        self.device_label = ttk.Label(top, text='未连接', style='Muted.TLabel')
        self.device_label.grid(row=1, column=0, columnspan=7, sticky='w',
                               pady=(8, 0))

        # quick buttons
        quick = ttk.LabelFrame(outer, text='快速定位', padding=10)
        quick.pack(fill='x', pady=(8, 0))
        for col, (text, cmd, style) in enumerate([
            ('鼠标 LOGO（0x04）', lambda: self.select_led('mouse', 4), 'TButton'),
            ('充电底座（0x05）', lambda: self.select_led('dock', 5), 'TButton'),
            ('闪一下鼠标 LOGO', self.flash_mouse, 'Accent.TButton'),
            ('闪一下底座灯', self.flash_dock, 'TButton'),
        ]):
            ttk.Button(quick, text=text, command=cmd, style=style).grid(
                row=0, column=col, padx=(0, 8), sticky='w')

        # middle: brightness + color
        mid = ttk.Frame(outer)
        mid.pack(fill='both', expand=True, pady=(8, 0))
        mid.columnconfigure(0, weight=1)
        mid.columnconfigure(1, weight=1)

        bright = ttk.LabelFrame(mid, text='亮度', padding=10)
        bright.grid(row=0, column=0, sticky='nsew', padx=(0, 6))
        row = 0
        ttk.Label(bright, text='LED ID').grid(row=row, column=0, sticky='w')
        self.led_var = tk.StringVar(value='4')
        ttk.Entry(bright, textvariable=self.led_var, width=8).grid(
            row=row, column=1, sticky='w', padx=(6, 12))
        ttk.Label(bright, text='0x04 = 鼠标 LOGO · 0x05 = 底座',
                  style='Muted.TLabel').grid(row=row, column=2, columnspan=3,
                                             sticky='w')
        row += 1
        self.bright_var = tk.DoubleVar(value=0.0)
        self.bright_scale = ttk.Scale(
            bright, from_=0, to=255, variable=self.bright_var,
            command=lambda _v: self.bright_value.configure(
                text='%d / 255' % int(float(self.bright_var.get()))))
        self.bright_scale.grid(row=row, column=0, columnspan=4, sticky='ew',
                               pady=(8, 2))
        bright.columnconfigure(0, weight=1)
        self.bright_value = ttk.Label(bright, text='0 / 255', style='Accent.TLabel')
        self.bright_value.grid(row=row, column=4, sticky='e', padx=(8, 0))
        row += 1
        btns = ttk.Frame(bright)
        btns.grid(row=row, column=0, columnspan=5, sticky='w', pady=(8, 0))
        ttk.Button(btns, text='读取', command=self.read_led).pack(side='left')
        ttk.Button(btns, text='写入亮度', style='Accent.TButton',
                   command=self.write_brightness).pack(side='left', padx=6)
        ttk.Button(btns, text='设为 0（关闭）',
                   command=lambda: self.set_brightness(0)).pack(side='left', padx=(0, 6))
        ttk.Button(btns, text='设为 255（最亮）',
                   command=lambda: self.set_brightness(255)).pack(side='left')
        row += 1
        self.reading = ttk.Label(bright, text='等待读取…', style='Muted.TLabel',
                                 justify='left')
        self.reading.grid(row=row, column=0, columnspan=5, sticky='w', pady=(8, 0))

        col = ttk.LabelFrame(mid, text='颜色 / 效果', padding=10)
        col.grid(row=0, column=1, sticky='nsew', padx=(6, 0))
        ttk.Label(col, text='R').grid(row=0, column=0, sticky='w')
        self.r_var = tk.StringVar(value='0')
        ttk.Entry(col, textvariable=self.r_var, width=5).grid(row=0, column=1, padx=(4, 10))
        ttk.Label(col, text='G').grid(row=0, column=2, sticky='w')
        self.g_var = tk.StringVar(value='255')
        ttk.Entry(col, textvariable=self.g_var, width=5).grid(row=0, column=3, padx=(4, 10))
        ttk.Label(col, text='B').grid(row=0, column=4, sticky='w')
        self.b_var = tk.StringVar(value='0')
        ttk.Entry(col, textvariable=self.b_var, width=5).grid(row=0, column=5, padx=(4, 10))
        self.swatch = tk.Canvas(col, width=54, height=28, bg='#00ff00',
                                highlightthickness=1, highlightbackground=BORDER)
        self.swatch.grid(row=0, column=6, padx=(8, 0))
        for var in (self.r_var, self.g_var, self.b_var):
            var.trace_add('write', lambda *_: self._update_swatch())

        row = 1
        ttk.Button(col, text='写入静态色', style='Accent.TButton',
                   command=self.write_color).grid(row=row, column=0, columnspan=3,
                                                  sticky='w', pady=(10, 0))
        ttk.Label(col, text='写的是当前 LED ID', style='Muted.TLabel').grid(
            row=row, column=3, columnspan=4, sticky='w', pady=(10, 0))

        row = 2
        ttk.Label(col, text='效果').grid(row=row, column=0, sticky='w', pady=(12, 0))
        self.effect_var = tk.StringVar(value='静态 0x01')
        self.effect_box = ttk.Combobox(
            col, textvariable=self.effect_var, state='readonly', width=16,
            values=['关闭 0x00', '静态 0x01', '呼吸 0x02', '光谱 0x03',
                    '波浪 0x04', '反应 0x05', '星光 0x07', '自定义 0x08'])
        self.effect_box.grid(row=row, column=1, columnspan=3, sticky='w',
                             padx=(6, 0), pady=(12, 0))
        ttk.Button(col, text='写入效果', command=self.write_effect).grid(
            row=row, column=4, columnspan=2, sticky='w', padx=(6, 0), pady=(12, 0))

        row = 3
        ttk.Label(col, text='说明：静态色用上面的 RGB；其它效果按 id 下发。',
                  style='Muted.TLabel', wraplength=360, justify='left').grid(
            row=row, column=0, columnspan=7, sticky='w', pady=(10, 0))

        # raw
        raw = ttk.LabelFrame(outer, text='原始命令', padding=10)
        raw.pack(fill='x', pady=(8, 0))
        ttk.Label(raw, text='Class').grid(row=0, column=0, sticky='w')
        self.cls_var = tk.StringVar(value='0F')
        ttk.Entry(raw, textvariable=self.cls_var, width=6).grid(row=0, column=1, padx=(4, 10))
        ttk.Label(raw, text='ID').grid(row=0, column=2, sticky='w')
        self.cid_var = tk.StringVar(value='84')
        ttk.Entry(raw, textvariable=self.cid_var, width=6).grid(row=0, column=3, padx=(4, 10))
        ttk.Label(raw, text='Args').grid(row=0, column=4, sticky='w')
        self.args_var = tk.StringVar(value='01 04 00')
        ttk.Entry(raw, textvariable=self.args_var, width=38).grid(
            row=0, column=5, padx=(4, 10), sticky='we')
        ttk.Label(raw, text='Size').grid(row=0, column=6, sticky='w')
        self.size_var = tk.StringVar(value='3')
        ttk.Entry(raw, textvariable=self.size_var, width=6).grid(row=0, column=7, padx=(4, 10))
        ttk.Button(raw, text='发送', command=self.send_raw).grid(row=0, column=8)
        raw.columnconfigure(5, weight=1)

        # log
        logf = ttk.LabelFrame(outer, text='日志', padding=10)
        logf.pack(fill='both', expand=True, pady=(8, 0))
        self.log = tk.Text(logf, height=10, bg=CARD, fg=FG, insertbackground=FG,
                           relief='flat', wrap='word', font=('Consolas', 9),
                           highlightthickness=1, highlightbackground=BORDER)
        self.log.pack(side='left', fill='both', expand=True)
        scroll = ttk.Scrollbar(logf, command=self.log.yview)
        scroll.pack(side='right', fill='y')
        self.log.configure(yscrollcommand=scroll.set, state='disabled')

        # bottom status
        self.status = ttk.Label(outer, text='就绪', style='Muted.TLabel')
        self.status.pack(fill='x', pady=(8, 0))

    # ---------------- helpers ----------------
    def _log(self, text):
        stamp = __import__('datetime').datetime.now().strftime('%H:%M:%S')
        self.log.configure(state='normal')
        self.log.insert('end', '[%s] %s\n' % (stamp, text))
        self.log.see('end')
        self.log.configure(state='disabled')

    def _update_swatch(self):
        r = max(0, min(255, parse_hex(self.r_var.get(), 0)))
        g = max(0, min(255, parse_hex(self.g_var.get(), 0)))
        b = max(0, min(255, parse_hex(self.b_var.get(), 0)))
        self.swatch.configure(bg='#%02x%02x%02x' % (r, g, b))

    def _set_status(self, text, style='Muted.TLabel'):
        self.status.configure(text=text, style=style)

    def _led(self):
        return parse_hex(self.led_var.get(), 0)

    def _target_pids(self):
        target = self.target.get()
        if target.startswith('鼠标'):
            return (0x007C, 0x007D)
        if target.startswith('充电底座'):
            return (0x007E,)
        return (parse_hex(self.pid_var.get(), 0x007D),)

    def _find(self, pids):
        candidates = []
        for path in R.enumerate_hid():
            info = R.info(path)
            if not info or info['vid'] != 0x1532 or info['feature'] != 91:
                continue
            if info['pid'] in pids:
                candidates.append((path, info))
        return candidates

    def _open_for_pids(self, pids, prefer_dpi=False):
        candidates = self._find(pids)
        if not candidates:
            return None, None, None
        if prefer_dpi:
            for path, info in candidates:
                c = Z.RazerClient(path, timeout=0.4)
                frame, err = c.get(Z.Cmd.DPI_GET, 7)
                c.close()
                if frame is not None and err is None:
                    return path, info, True
        path, info = candidates[0]
        return path, info, False

    def connect(self):
        pids = self._target_pids()
        prefer = self.target.get().startswith('鼠标')
        path, info, _ = self._open_for_pids(pids, prefer_dpi=prefer)
        if not path:
            self.device_label.configure(
                text='未找到 PID %s 的控制接口' % ', '.join('%04X' % p for p in pids),
                style='Danger.TLabel')
            self._log('连接失败：未找到目标 PID 的控制接口')
            self.client = None
            return
        if self.client:
            try:
                self.client.close()
            except Exception:
                pass
        self.client = Z.RazerClient(path, timeout=0.8)
        self.info = info
        self.path = path
        text = '%04X:%04X  %s  feat=91  %s' % (
            info['vid'], info['pid'], info['product'],
            path.split('#')[-1][:44])
        self.device_label.configure(text=text, style='Accent.TLabel')
        self.pid_var.set('%04X' % info['pid'])
        if info['pid'] in (0x007C, 0x007D):
            self.led_var.set('4')
            self.target.set('鼠标 (007D/007C)')
        elif info['pid'] == 0x007E:
            self.led_var.set('5')
            self.target.set('充电底座 (007E)')
        self._log('已连接：%s' % text)
        self._set_status('已连接，可读取/写入灯效', 'Accent.TLabel')
        self.read_led(silent=True)

    def _require(self):
        if not self.client:
            self._log('请先点击「连接 / 刷新」')
            self._set_status('未连接', 'Warn.TLabel')
            return False
        return True

    # ---------------- operations ----------------
    def read_led(self, silent=False):
        if not self._require():
            return
        led = self._led()
        frame, err = self.client.xfer(0x0F, 0x84, bytes([0x01, led, 0x00]), 3)
        if frame is None:
            text = '读取失败：%s' % (err or '无应答')
            self.reading.configure(text=text, style='Danger.TLabel')
            if not silent:
                self._log('brightness 0x%02X: %s' % (led, text))
            return
        raw = frame[10]
        bright = 'raw=0x%02X (%d%%) args=%s' % (
            raw, round(raw * 100 / 255), hs(frame[8:11]))
        frame, err2 = self.client.xfer(0x0F, 0x82, bytes([0x01, led]), 2)
        if frame is None:
            effect = 'effect: FAIL %s' % (err2 or '无应答')
        else:
            effect = 'effect: %s' % hs(frame[8:14])
        self.reading.configure(text='LED 0x%02X  %s\n%s' % (led, bright, effect),
                               style='Muted.TLabel')
        if not silent:
            self._log('LED 0x%02X  %s  %s' % (led, bright, effect))
        self._set_status('读取完成', 'Accent.TLabel')

    def set_brightness(self, value):
        if not self._require():
            return
        led = self._led()
        value = max(0, min(255, int(value)))
        frame, err = self.client.xfer(0x0F, 0x04, bytes([0x01, led, value]), 3)
        if frame is None:
            self._log('亮度写入失败：%s' % (err or '无应答'))
            self._set_status('写入失败', 'Danger.TLabel')
            return
        self.bright_var.set(value)
        self.bright_value.configure(text='%d / 255' % value)
        self._log('LED 0x%02X 亮度 -> %d/255 (%d%%)  %s' % (
            led, value, round(value * 100 / 255), err or 'ok'))
        self._set_status('亮度已写入', 'Accent.TLabel')
        self.read_led(silent=True)

    def write_brightness(self):
        self.set_brightness(int(float(self.bright_var.get())))

    def write_color(self):
        if not self._require():
            return
        led = self._led()
        r = max(0, min(255, parse_hex(self.r_var.get(), 0)))
        g = max(0, min(255, parse_hex(self.g_var.get(), 0)))
        b = max(0, min(255, parse_hex(self.b_var.get(), 0)))
        payload = bytes([0x01, led, 0x01, 0x00, 0x00, 0x01, r, g, b])
        frame, err = self.client.xfer(0x0F, 0x02, payload, len(payload))
        self._log('LED 0x%02X 静态色 #%02X%02X%02X -> %s' % (
            led, r, g, b, err or 'ok'))
        if frame is None:
            self._set_status('颜色写入失败', 'Danger.TLabel')
        else:
            self._set_status('颜色已写入', 'Accent.TLabel')
            self.read_led(silent=True)

    def write_effect(self):
        if not self._require():
            return
        led = self._led()
        text = self.effect_var.get()
        effect = parse_hex(text.split('0x')[-1], 0)
        if effect == 0x01:
            # Static: include color.
            self.write_color()
            return
        payload = bytes([0x01, led, effect, 0x00, 0x00, 0x00])
        frame, err = self.client.xfer(0x0F, 0x02, payload, len(payload))
        self._log('LED 0x%02X 效果 0x%02X -> %s' % (led, effect, err or 'ok'))
        if frame is None:
            self._set_status('效果写入失败', 'Danger.TLabel')
        else:
            self._set_status('效果已写入', 'Accent.TLabel')
            self.read_led(silent=True)

    def read_both(self):
        results = []
        for label, pids, led in [
                ('鼠标 LOGO', (0x007C, 0x007D), 4),
                ('充电底座', (0x007E,), 5)]:
            path, info, _ = self._open_for_pids(pids, prefer_dpi=(label == '鼠标 LOGO'))
            if not path:
                results.append('%s: 未找到' % label)
                continue
            c = Z.RazerClient(path, timeout=0.6)
            frame, err = c.xfer(0x0F, 0x84, bytes([0x01, led, 0x00]), 3)
            if frame is None:
                results.append('%s: 读取失败 %s' % (label, err or '无应答'))
            else:
                raw = frame[10]
                results.append('%s 0x%02X: raw=0x%02X (%d%%)' % (
                    label, led, raw, round(raw * 100 / 255)))
            c.close()
        for line in results:
            self._log(line)
        self._set_status('两个灯区读取完成', 'Accent.TLabel')

    def send_raw(self):
        if not self._require():
            return
        cls = parse_hex(self.cls_var.get(), 0)
        cid = parse_hex(self.cid_var.get(), 0)
        try:
            args = bytes.fromhex(self.args_var.get().replace(' ', '').replace(',', ''))
        except ValueError:
            self._log('Args 不是合法十六进制')
            return
        size = parse_hex(self.size_var.get(), len(args))
        frame, err = self.client.xfer(cls, cid, args, size)
        if frame is None:
            self._log('raw %02X/%02X %s -> %s' % (cls, cid, hs(args), err or '无应答'))
        else:
            self._log('raw %02X/%02X %s -> %s args=%s' % (
                cls, cid, hs(args), err or 'ok', hs(frame[8:8 + min(24, len(args) + 8)])))

    # ---------------- quick tests ----------------
    def select_led(self, target, led):
        if target == 'mouse':
            self.target.set('鼠标 (007D/007C)')
        elif target == 'dock':
            self.target.set('充电底座 (007E)')
        self.led_var.set(str(led))
        self.connect()

    def _flash(self, label, pids, led, on_value, restore_default):
        path, info, _ = self._open_for_pids(pids, prefer_dpi=(label == '鼠标 LOGO'))
        if not path:
            self._log('%s：未找到设备' % label)
            return
        c = Z.RazerClient(path, timeout=0.7)
        frame, err = c.xfer(0x0F, 0x84, bytes([0x01, led, 0x00]), 3)
        restore = restore_default
        if frame is not None:
            restore = frame[10]
        frame, err = c.xfer(0x0F, 0x04, bytes([0x01, led, on_value]), 3)
        if frame is None:
            self._log('%s：写入失败 %s' % (label, err or '无应答'))
            c.close()
            return
        self._log('%s：LED 0x%02X 设为 %d/255，2.5 秒后恢复为 %d/255' % (
            label, led, on_value, restore))
        self._set_status('%s 测试中…' % label, 'Accent.TLabel')
        self.flash_clients.append(c)

        def restore_now():
            try:
                c.xfer(0x0F, 0x04, bytes([0x01, led, restore]), 3)
            except Exception:
                pass
            try:
                c.close()
            except Exception:
                pass
            if c in self.flash_clients:
                self.flash_clients.remove(c)
            self._log('%s：已恢复为 %d/255' % (label, restore))
            self._set_status('已恢复', 'Muted.TLabel')

        self.after(2500, restore_now)

    def flash_mouse(self):
        self._flash('鼠标 LOGO', (0x007C, 0x007D), 4, 0xFF, 0x00)

    def flash_dock(self):
        # The dock normally sits at full brightness; blink it off, then restore.
        self._flash('充电底座', (0x007E,), 5, 0x00, 0xFF)

    def clear_log(self):
        self.log.configure(state='normal')
        self.log.delete('1.0', 'end')
        self.log.configure(state='disabled')


if __name__ == '__main__':
    app = LedLab()
    app.mainloop()
