# capture test: click at window-relative coords, inject a key, screenshot
param(
  [int]$X = 418,
  [int]$Y = 263,
  [int]$Key = 0x4B,
  [string]$Out = "tmp_cap.png",
  [int]$Wait = 600
)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;using System.Text;using System.Runtime.InteropServices;
public class CP2 {
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
  public static void Key(byte vk){
    keybd_event(vk,0,0,IntPtr.Zero); System.Threading.Thread.Sleep(50);
    keybd_event(vk,0,2,IntPtr.Zero);
  }
}
"@
$d = 'D:\tools\ctf\script\DS_Desktop\aday\razer-driver'
$p = Get-Process RazerControl
$h = [CP2]::Find([uint32]$p.Id)
$r = New-Object CP2+RECT
[CP2]::GetWindowRect($h, [ref]$r) | Out-Null
$L=$r.L; $T=$r.T; $w=$r.R-$r.L; $hh=$r.B-$r.T
[CP2]::SetWindowPos($h, [IntPtr](-1), $L, $T, $w, $hh, 0x0040) | Out-Null
[CP2]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 400
[CP2]::Click(($L + $X), ($T + $Y))
Start-Sleep -Milliseconds $Wait
[CP2]::Key([byte]$Key)
Start-Sleep -Milliseconds 600
$bmp = New-Object System.Drawing.Bitmap($w, $hh)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
[CP2]::PrintWindow($h, $hdc, 2) | Out-Null
$g.ReleaseHdc($hdc); $g.Dispose()
$bmp.Save("$d\docs\$Out", [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
[CP2]::SetWindowPos($h, [IntPtr](-2), $L, $T, $w, $hh, 0x0040) | Out-Null
Write-Host ("clicked {0},{1} key=0x{2:X2} -> {3}" -f $X,$Y,$Key,$Out)
