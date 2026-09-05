# Tier 2: the Chromium address bar.
#
# The omnibox keeps a trailing inline-autocomplete selection, so the first
# synthetic Backspace deletes the selection instead of a character and the edit
# lands short — `hoongf` becomes `hoồng`. This is the defect
# docs/decisions/0003 records on macOS, and it reproduces on Windows.
#
# Reads the address bar back through UI Automation, because it is not a classic
# EDIT control and WM_GETTEXT returns nothing useful for it.

$ErrorActionPreference = 'Stop'

Add-Type @'
using System;
using System.Runtime.InteropServices;
public class E {
    [StructLayout(LayoutKind.Sequential)]
    public struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags;
                               public uint time; public IntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Explicit)]
    public struct INPUT { [FieldOffset(0)] public uint type; [FieldOffset(8)] public KEYBDINPUT ki; }
    [DllImport("user32.dll", SetLastError=true)]
    public static extern uint SendInput(uint n, INPUT[] p, int cb);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    // INPUT is 40 bytes on x64; Marshal.SizeOf on this trimmed declaration says
    // 32, and SendInput rejects a wrong cbSize outright rather than doing
    // anything - which reads exactly like the hook not firing.
    public static int Cb { get { return IntPtr.Size == 8 ? 40 : 28; } }

    static void Key(ushort vk, bool up) {
        INPUT[] one = new INPUT[1];
        one[0].type = 1; one[0].ki.wVk = vk; one[0].ki.dwFlags = up ? 2u : 0u;
        SendInput(1, one, Cb);
        System.Threading.Thread.Sleep(45);
    }
    public static void Type(string s) {
        foreach (char c in s) { Key((ushort)Char.ToUpper(c), false); Key((ushort)Char.ToUpper(c), true); }
    }
    public static void Chord(ushort mod, ushort vk) {
        Key(mod, false); Key(vk, false); Key(vk, true); Key(mod, true);
        System.Threading.Thread.Sleep(250);
    }
}
'@

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$edge = $null
try {
    $edge = Start-Process msedge -PassThru -ArgumentList "about:blank"
    Start-Sleep -Seconds 5
    [void][E]::SetForegroundWindow($edge.MainWindowHandle)
    Start-Sleep -Seconds 2

    # Ctrl+L focuses the address bar. Ctrl+A then Delete clears whatever is there.
    [E]::Chord(0x11, 0x4C)   # Ctrl+L
    [E]::Chord(0x11, 0x41)   # Ctrl+A
    [E]::Type([string][char]0x08)  # not reliable; use VK_DELETE below instead
    Start-Sleep -Milliseconds 300

    [E]::Type("hoongf")
    Start-Sleep -Milliseconds 800

    # Read the focused element's value through UI Automation.
    $focused = [System.Windows.Automation.AutomationElement]::FocusedElement
    $got = ""
    if ($focused) {
        $pattern = $null
        if ($focused.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
            $got = $pattern.Current.Value
        }
    }

    $expected = "h" + [string][char]0x1ED3 + "ng"   # hồng
    $bad      = "ho" + [string][char]0x1ED3 + "ng"  # hoồng - the omnibox bug
    $hex = (( $got.ToCharArray() | ForEach-Object { '{0:X4}' -f [int]$_ } ) -join ' ')

    Write-Output "TYPED:    hoongf  (into the Edge address bar)"
    Write-Output "GOT:      '$got'"
    Write-Output "HEX:      $hex"
    if ($got -like "*$expected") { Write-Output "RESULT: PASS - the omnibox guard held" }
    elseif ($got -like "*$bad*")  { Write-Output "RESULT: FAIL - the omnibox bug reproduced (hoong -> hoong with a stray o)" }
    else                          { Write-Output "RESULT: UNEXPECTED - read the hex above" }
}
finally {
    if ($edge) { Stop-Process -Id $edge.Id -Force -ErrorAction SilentlyContinue }
    Get-Process -Name msedge -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}
