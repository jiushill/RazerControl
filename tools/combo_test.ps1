# Press Ctrl+Alt+R via injected input (combo hotkey test)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;using System.Text;using System.Runtime.InteropServices;
public class CB {
  public delegate bool EP(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EP cb, IntPtr l);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte sc, uint f, IntPtr e);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; }
  public static IntPtr Find(uint wantPid){
    IntPtr found = IntPtr.Zero;
    EnumWindows((h,l)=>{ uint pid; GetWindowThreadProcessId(h,out pid);
      if(pid==wantPid){ var c=new StringBuilder(128); GetClassName(h,c,128);
        if(c.ToString()=="Window Class"){ found=h; return false; } }
      return true;}, IntPtr.Zero);
    return found; }
  public static void Click(int x,int y){
    SetCursorPos(x,y); System.Threading.Thread.Sleep(90);
    mouse_event(0x0002,0,0,0,IntPtr.Zero); System.Threading.Thread.Sleep(60);
    mouse_event(0x0004,0,0,0,IntPtr.Zero);
  }
  public static void Down(byte vk){ keybd_event(vk,0,0,IntPtr.Zero); }
  public static void Up(byte vk){ keybd_event(vk,0,2,IntPtr.Zero); }
}
"@
$d = 'D:\tools\ctf\script\DS_Desktop\aday\razer-driver'
$p = Get-Process RazerControl
$h = [CB]::Find([uint32]$p.Id)
$r = New-Object CB+RECT
[CB]::GetWindowRect($h, [ref]$r) | Out-Null
$L=$r.L; $T=$r.T; $w=$r.R-$r.L; $hh=$r.B-$r.T
[CB]::SetWindowPos($h, [IntPtr](-1), $L, $T, $w, $hh, 0x0040) | Out-Null
[CB]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 500

# 点「录制热键」按钮（窗口内 ~471,262）
[CB]::Click(($L + 471), ($T + 262))
Start-Sleep -Milliseconds 600

# Ctrl + Alt + R
[CB]::Down(0x11); Start-Sleep -Milliseconds 80
[CB]::Down(0x12); Start-Sleep -Milliseconds 80
[CB]::Down(0x52); Start-Sleep -Milliseconds 60
[CB]::Up(0x52);   Start-Sleep -Milliseconds 60
[CB]::Up(0x12);   Start-Sleep -Milliseconds 60
[CB]::Up(0x11);   Start-Sleep -Milliseconds 400

$bmp = New-Object System.Drawing.Bitmap($w, $hh)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
[CB]::PrintWindow($h, $hdc, 2) | Out-Null
$g.ReleaseHdc($hdc); $g.Dispose()
$bmp.Save("$d\docs\tmp_combo.png", [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
[CB]::SetWindowPos($h, [IntPtr](-2), $L, $T, $w, $hh, 0x0040) | Out-Null
Write-Host "combo test done"
