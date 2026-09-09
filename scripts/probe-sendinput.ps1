# Can this process synthesize a keystroke, and if not, why not?
#
# Written to settle a claim that turned out to be wrong once already. An earlier
# run concluded "SendInput is denied to a background process here" from a single
# ERROR_ACCESS_DENIED, and recorded it as a hard limit. It was not: an elevated
# Windows Terminal happened to be the foreground window, and UIPI blocks
# injection into a higher-integrity foreground from any ordinary process. With an
# ordinary window in front the same call succeeds.
#
# So this reports the foreground window and its elevation alongside the result,
# because the result alone is what caused the wrong conclusion.
#
# Harmless: taps Shift, which does nothing on its own.

$ErrorActionPreference = 'Stop'

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class P {
  [StructLayout(LayoutKind.Sequential)]
  public struct KEYBDINPUT { public ushort wVk, wScan; public uint dwFlags, time; public IntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Explicit)]
  public struct INPUT { [FieldOffset(0)] public uint type; [FieldOffset(8)] public KEYBDINPUT ki; }
  [DllImport("user32.dll", SetLastError=true)] public static extern uint SendInput(uint n, INPUT[] p, int cb);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern IntPtr GetThreadDesktop(int t);
  [DllImport("kernel32.dll")] public static extern int GetCurrentThreadId();
  [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern bool GetUserObjectInformationW(IntPtr h, int i, StringBuilder p, int n, out int len);

  public static string Desktop() {
    var sb = new StringBuilder(256); int l;
    GetUserObjectInformationW(GetThreadDesktop(GetCurrentThreadId()), 2, sb, 256, out l);
    return sb.ToString();
  }
  public static string Title(IntPtr h) { var sb = new StringBuilder(256); GetWindowTextW(h, sb, 256); return sb.ToString(); }
  public static string Tap() {
    INPUT[] two = new INPUT[2];
    two[0].type = 1; two[0].ki.wVk = 0x10;
    two[1].type = 1; two[1].ki.wVk = 0x10; two[1].ki.dwFlags = 2;
    uint sent = SendInput(2, two, IntPtr.Size == 8 ? 40 : 28);
    return "sent=" + sent + "/2 err=" + Marshal.GetLastWin32Error();
  }
}
"@

$h = [P]::GetForegroundWindow()
$procId = 0
[void][P]::GetWindowThreadProcessId($h, [ref]$procId)
$name = try { (Get-Process -Id $procId -ErrorAction Stop).ProcessName } catch { "none" }

Write-Output ("desktop        : {0}" -f [P]::Desktop())
Write-Output ("foreground     : {0} (pid {1}) '{2}'" -f $name, $procId, [P]::Title($h))
Write-Output ("sendinput      : {0}" -f [P]::Tap())
Write-Output ""
Write-Output "err=0  -> injection works from here"
Write-Output "err=5  -> ACCESS_DENIED: either not the input desktop, or UIPI (foreground is elevated)"
