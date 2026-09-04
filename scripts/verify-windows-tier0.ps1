# Tier 0: do modifiers reach the hook at all?
#
# GlowKey's hook thread never has keyboard focus. If its per-thread key state is
# not updated, GetKeyState reports Shift/Ctrl as never held, and then the shortcut
# filter and BOTH Ctrl+Shift hotkeys are silently dead while ordinary typing looks
# completely fine. That is the worst kind of bug to find late, so it is checked
# first.
#
# Ctrl+Shift+E is the per-app toggle. A TOGGLE line in the log means the modifiers
# arrived. No line means they did not.

$ErrorActionPreference = 'Stop'

Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public class M {
    [StructLayout(LayoutKind.Sequential)]
    public struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags;
                               public uint time; public IntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Explicit)]
    public struct INPUT { [FieldOffset(0)] public uint type; [FieldOffset(8)] public KEYBDINPUT ki; }
    [DllImport("user32.dll", SetLastError=true)]
    public static extern uint SendInput(uint n, INPUT[] p, int cb);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr FindWindowEx(IntPtr p, IntPtr c, string cls, string win);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)]
    public static extern int SendMessageW(IntPtr h, uint msg, IntPtr wp, StringBuilder lp);
    public static int Cb { get { return IntPtr.Size == 8 ? 40 : 28; } }

    static void Key(ushort vk, bool up) {
        INPUT[] one = new INPUT[1];
        one[0].type = 1; one[0].ki.wVk = vk; one[0].ki.dwFlags = up ? 2u : 0u;
        SendInput(1, one, Cb);
        System.Threading.Thread.Sleep(30);
    }
    // Ctrl and Shift down, the key, then both up — the way a person presses it.
    public static void Chord(ushort vk) {
        Key(0x11, false);  // VK_CONTROL
        Key(0x10, false);  // VK_SHIFT
        Key(vk,   false);
        Key(vk,   true);
        Key(0x10, true);
        Key(0x11, true);
        System.Threading.Thread.Sleep(200);
    }
    public static void Type(string s) {
        foreach (char c in s) { Key((ushort)Char.ToUpper(c), false); Key((ushort)Char.ToUpper(c), true); }
    }
    public static string GetText(IntPtr edit) {
        var sb = new StringBuilder(4096);
        SendMessageW(edit, 0x000D, (IntPtr)4096, sb);
        return sb.ToString();
    }
}
'@

$glow = $null; $pad = $null
try {
    $glow = Start-Process -FilePath (Join-Path (Get-Location) "target/release/GlowKey.exe") -PassThru
    Start-Sleep -Seconds 2
    $pad = Start-Process notepad -PassThru
    Start-Sleep -Seconds 2
    [void][M]::SetForegroundWindow($pad.MainWindowHandle)
    Start-Sleep -Seconds 2
    $edit = [M]::FindWindowEx($pad.MainWindowHandle, [IntPtr]::Zero, "Edit", $null)

    # Baseline: Vietnamese is on and working here.
    [M]::Type("hoongf")
    Start-Sleep -Milliseconds 400
    Write-Output "BEFORE-TOGGLE: '$([M]::GetText($edit))'  (expect hong-with-tone)"

    # Ctrl+Shift+E — the per-app toggle. Should disable Vietnamese for notepad.
    [M]::Chord(0x45)   # 'E'
    Start-Sleep -Milliseconds 400

    # Type again. If the toggle landed, this stays raw ASCII.
    [M]::Type("hoongf")
    Start-Sleep -Milliseconds 400
    Write-Output "AFTER-TOGGLE:  '$([M]::GetText($edit))'"

    # Ctrl+Shift+Space — the VN/EN mode toggle.
    [M]::Chord(0x20)
    Start-Sleep -Milliseconds 400
}
finally {
    if ($pad)  { Stop-Process -Id $pad.Id  -Force -ErrorAction SilentlyContinue }
    if ($glow) { Stop-Process -Id $glow.Id -Force -ErrorAction SilentlyContinue }
}
