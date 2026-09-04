# Tier 1 smoke test: does GlowKey actually transform keystrokes in a real app?
#
# Launches Notepad, focuses it, synthesizes keystrokes with SendInput, and reads
# the edit control's text back. Everything is killed on the way out, including on
# failure, because the thing under test is a global keyboard hook.

$ErrorActionPreference = 'Stop'

Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public class W {
    [StructLayout(LayoutKind.Sequential)]
    public struct KEYBDINPUT {
        public ushort wVk; public ushort wScan; public uint dwFlags;
        public uint time; public IntPtr dwExtraInfo;
    }
    [StructLayout(LayoutKind.Explicit)]
    public struct INPUT {
        [FieldOffset(0)] public uint type;
        [FieldOffset(8)] public KEYBDINPUT ki;
    }
    [DllImport("user32.dll", SetLastError=true)]
    public static extern uint SendInput(uint n, INPUT[] p, int cb);

    // INPUT on x64 is 40 bytes: a 4-byte type, 4 bytes of padding, then a union
    // whose largest member (MOUSEINPUT) is 32. Marshal.SizeOf on the trimmed
    // declaration above reports 32, and SendInput rejects a wrong cbSize
    // outright rather than doing anything - which reads exactly like the hook
    // not firing.
    public static int Cb { get { return IntPtr.Size == 8 ? 40 : 28; } }
    public static uint LastError { get { return (uint)Marshal.GetLastWin32Error(); } }
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr FindWindowEx(IntPtr p, IntPtr c, string cls, string win);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)]
    public static extern int SendMessageW(IntPtr h, uint msg, IntPtr wp, StringBuilder lp);
    [DllImport("user32.dll")]
    public static extern int SendMessageW(IntPtr h, uint msg, IntPtr wp, IntPtr lp);

    // Types a run of ASCII letters as real virtual-key presses, the way a person
    // does. NOT KEYEVENTF_UNICODE: the point is to exercise the layout path and
    // the hook, and a unicode-injected char would bypass exactly what is under
    // test.
    public static void TypeKeys(string s) {
        foreach (char c in s) {
            ushort vk = (ushort)Char.ToUpper(c);
            INPUT[] two = new INPUT[2];
            two[0].type = 1; two[0].ki.wVk = vk; two[0].ki.dwFlags = 0;
            two[1].type = 1; two[1].ki.wVk = vk; two[1].ki.dwFlags = 2; // KEYUP
            uint sent = SendInput(2, two, Cb);
            if (sent != 2) { throw new Exception("SendInput sent " + sent + "/2, error " + Marshal.GetLastWin32Error()); }
            System.Threading.Thread.Sleep(40);
        }
    }
    // One key by virtual-key code, for Backspace and Space.
    public static void TapVk(ushort vk) {
        INPUT[] two = new INPUT[2];
        two[0].type = 1; two[0].ki.wVk = vk; two[0].ki.dwFlags = 0;
        two[1].type = 1; two[1].ki.wVk = vk; two[1].ki.dwFlags = 2;
        SendInput(2, two, Cb);
        System.Threading.Thread.Sleep(60);
    }
    public static void SetText(IntPtr edit, string s) {
        SendMessageW(edit, 0x000C /* WM_SETTEXT */, IntPtr.Zero, new StringBuilder(s));
        System.Threading.Thread.Sleep(80);
    }
    public static string GetText(IntPtr edit) {
        var sb = new StringBuilder(4096);
        SendMessageW(edit, 0x000D /* WM_GETTEXT */, (IntPtr)4096, sb);
        return sb.ToString();
    }
}
'@

$glow = $null
$pad  = $null
$results = @()

function Check($name, $keys, $expected, $edit) {
    # Clearing the document with WM_SETTEXT happens behind GlowKey's back, which
    # is precisely the blind model's one invariant being violated: the engine
    # still believes it rendered the previous word at the caret. Home is a
    # caret move, and the ladder flushes on those - so this puts GlowKey back in
    # step with a document the harness just rewrote.
    [W]::SetText($edit, "")
    Start-Sleep -Milliseconds 150
    [W]::TapVk(0x24)   # VK_HOME - makes the ladder flush
    Start-Sleep -Milliseconds 150
    foreach ($k in $keys) {
        if ($k -eq "BS")    { [W]::TapVk(8) }
        elseif ($k -eq "SP") { [W]::TapVk(32) }
        else { [W]::TypeKeys($k) }
    }
    Start-Sleep -Milliseconds 400
    $got = [W]::GetText($edit)
    $hex = (( $got.ToCharArray() | ForEach-Object { '{0:X4}' -f [int]$_ } ) -join ' ')
    $ok = ($got -eq $expected)
    Write-Output ("{0,-42} {1,-6} got={2,-12} hex={3}" -f $name, $(if($ok){"PASS"}else{"FAIL"}), $got, $hex)
}

try {
    $glow = Start-Process -FilePath (Join-Path (Get-Location) "target/release/GlowKey.exe") -PassThru
    Start-Sleep -Seconds 2
    $pad = Start-Process notepad -PassThru
    Start-Sleep -Seconds 2
    [void][W]::SetForegroundWindow($pad.MainWindowHandle)
    Start-Sleep -Seconds 2
    $edit = [W]::FindWindowEx($pad.MainWindowHandle, [IntPtr]::Zero, "Edit", $null)
    if ($edit -eq [IntPtr]::Zero) { Write-Output "no edit control"; exit 1 }

    $o_circ_grave = [string][char]0x1ED3   # o with circumflex + grave
    $o_circ       = [string][char]0x00F4   # o with circumflex

    Check "hoongf -> hong (tone)"            @("hoongf")                 ("h"+$o_circ_grave+"ng")  $edit
    Check "hoongf BS z -> hon (mid-word)"    @("hoongf","BS","z")        ("h"+$o_circ+"n")         $edit
    Check "hoongf SP BS z -> hong (boundary)" @("hoongf","SP","BS","z")  ("h"+$o_circ+"ng")        $edit
    Check "exit SP -> exit (auto-fix)"       @("exit","SP")              "exit "                   $edit
    Check "vieejt -> viet (tone on e)"       @("vieejt")                 ("vi"+[string][char]0x1EC7+"t") $edit
}
finally {
    if ($pad)  { Stop-Process -Id $pad.Id  -Force -ErrorAction SilentlyContinue }
    if ($glow) { Stop-Process -Id $glow.Id -Force -ErrorAction SilentlyContinue }
}
