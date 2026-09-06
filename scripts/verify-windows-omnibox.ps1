# **RUN THIS ONLY WHEN YOU ARE NOT USING THE MACHINE.**
#
# It takes focus, drives your Edge window, and types into the address bar. That
# is not a side effect, it is the thing being tested - the bug only exists when a
# real keystroke reaches a real omnibox. Nothing is committed (no Enter is ever
# sent, and Escape restores the URL), but it will interrupt whatever you are
# doing, and it was interrupting the author of this comment when he wrote it.
#
# Known limitation, unresolved: focusing the address bar is unreliable while a
# page has grabbed the keyboard. A YouTube player swallowed Ctrl+L, Alt+D and F6
# across several attempts. Open a plain tab (about:blank) before running, or
# focus the address bar by hand and comment out the focus block.
#
# Tier 2, the address bar: does `hoongf` produce `hong` in a Chromium omnibox?
#
# The one check no unit test can stand in for. GlowKey emits the correct diff
# either way - the engine suite proves that - and the whole question is whether
# the host applies it correctly. The omnibox keeps a trailing inline-autocomplete
# selection that eats the first synthetic Backspace, which is what turns `hong`
# into `hoong`, and the address-bar guard exists to clear it first.
#
# Reads the result back through UI Automation rather than a screenshot, so the
# answer is code points rather than something a human squints at.
#
#   .\scripts\verify-windows-omnibox.ps1
#
# Opens its OWN Edge window and closes it again, so nothing in the user's tabs is
# touched. It does steal focus for about ten seconds - it has to, since the thing
# under test is what a real keystroke does.
#
# GlowKey must be running, in Vietnamese mode, with Edge not excluded.

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class K {
  [StructLayout(LayoutKind.Sequential)]
  public struct KEYBDINPUT { public ushort wVk, wScan; public uint dwFlags, time; public IntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Explicit)]
  public struct INPUT { [FieldOffset(0)] public uint type; [FieldOffset(8)] public KEYBDINPUT ki; }
  [DllImport("user32.dll", SetLastError=true)] public static extern uint SendInput(uint n, INPUT[] p, int cb);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, IntPtr pid);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(int a, int b, bool attach);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("kernel32.dll")] public static extern int GetCurrentThreadId();

  /// Windows refuses SetForegroundWindow from a process that does not already
  /// own the foreground - which is every background process, including this one.
  /// Attaching to the current foreground thread's input queue lifts that, which
  /// is the standard way an automation tool takes focus. Detached again
  /// immediately: leaving two threads sharing an input queue is a good way to
  /// make somebody else's window stop responding.
  public static bool Focus(IntPtr target) {
    IntPtr fore = GetForegroundWindow();
    int foreThread = GetWindowThreadProcessId(fore, IntPtr.Zero);
    int self = GetCurrentThreadId();
    bool attached = false;
    if (foreThread != self) attached = AttachThreadInput(self, foreThread, true);
    ShowWindow(target, 9 /* SW_RESTORE */);
    BringWindowToTop(target);
    bool ok = SetForegroundWindow(target);
    if (attached) AttachThreadInput(self, foreThread, false);
    return ok;
  }
  static int Cb { get { return IntPtr.Size == 8 ? 40 : 28; } }

  public static void Tap(ushort vk) {
    INPUT[] two = new INPUT[2];
    two[0].type = 1; two[0].ki.wVk = vk;
    two[1].type = 1; two[1].ki.wVk = vk; two[1].ki.dwFlags = 2;
    if (SendInput(2, two, Cb) != 2) throw new Exception("SendInput refused vk " + vk + ", error " + Marshal.GetLastWin32Error());
    System.Threading.Thread.Sleep(90);
  }

  public static void Chord(ushort mod, ushort vk) {
    INPUT[] four = new INPUT[4];
    four[0].type = 1; four[0].ki.wVk = mod;
    four[1].type = 1; four[1].ki.wVk = vk;
    four[2].type = 1; four[2].ki.wVk = vk; four[2].ki.dwFlags = 2;
    four[3].type = 1; four[3].ki.wVk = mod; four[3].ki.dwFlags = 2;
    if (SendInput(4, four, Cb) != 4) throw new Exception("SendInput refused the chord, error " + Marshal.GetLastWin32Error());
    System.Threading.Thread.Sleep(250);
  }
}
"@

$VK_CONTROL = 0x11; $VK_L = 0x4C; $VK_W = 0x57; $VK_ESCAPE = 0x1B
$keys = @{ 'h' = 0x48; 'o' = 0x4F; 'n' = 0x4E; 'g' = 0x47; 'f' = 0x46 }

$edge = $null
try {
    # Drive the Edge window that already exists rather than opening one.
    #
    # A new window was the first instinct and it does not work: Edge hands the
    # command line to an existing process and exits, so the launched object
    # tracks no window at all, and a freshly painted window routes Ctrl+L to a
    # startup surface (the first run of this landed focus on an `MdTextButton`)
    # before it routes it to the omnibox.
    #
    # Nothing typed here is committed - no Enter is ever sent, and Escape at the
    # end restores whatever URL the address bar held.
    $edge = Get-Process msedge -ErrorAction SilentlyContinue |
        Where-Object { $_.MainWindowHandle -ne [IntPtr]::Zero } |
        Select-Object -First 1
    if (-not $edge) {
        Write-Output "RESULT: INCONCLUSIVE - no Edge window is open"
        return
    }

    # Three ways into the address bar, because a page can swallow the first.
    # A YouTube player had focus on one run and Ctrl+L never arrived; Alt+D and
    # F6 do not go through the page's key handling the same way.
    $VK_MENU = 0x12; $VK_D = 0x44; $VK_F6 = 0x75
    $class = ''
    foreach ($attempt in 1..3) {
        [void][K]::Focus($edge.MainWindowHandle)
        Start-Sleep -Milliseconds 600

        foreach ($route in 'ctrl-l', 'alt-d', 'f6') {
            switch ($route) {
                'ctrl-l' { [K]::Chord($VK_CONTROL, $VK_L) }
                'alt-d'  { [K]::Chord($VK_MENU, $VK_D) }
                'f6'     { [K]::Tap([uint16]$VK_F6) }
            }
            Start-Sleep -Milliseconds 700
            $class = [System.Windows.Automation.AutomationElement]::FocusedElement.Current.ClassName
            if ($class -eq 'OmniboxViewViews') {
                Write-Output "focus route   : $route"
                break
            }
        }
        if ($class -eq 'OmniboxViewViews') { break }
    }

    Write-Output "focused class : $class"
    if ($class -ne 'OmniboxViewViews') {
        Write-Output "RESULT: INCONCLUSIVE - the address bar did not take focus"
        return
    }

    # Empty it, so the omnibox offers an inline completion for what follows.
    [K]::Chord($VK_CONTROL, 0x41)
    [K]::Tap(0x2E)
    Start-Sleep -Milliseconds 400

    foreach ($ch in 'h', 'o', 'o', 'n', 'g', 'f') { [K]::Tap([uint16]$keys[$ch]) }
    Start-Sleep -Milliseconds 700

    # Read what actually landed, as code points.
    $el = [System.Windows.Automation.AutomationElement]::FocusedElement
    $vp = $null
    $text = if ($el.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$vp)) {
        $vp.Current.Value
    } else { $el.Current.Name }

    $hex = (($text.ToCharArray() | ForEach-Object { '{0:X4}' -f [int]$_ }) -join ' ')
    Write-Output "typed         : hoongf"
    Write-Output "got           : $text"
    Write-Output "code points   : $hex"
    Write-Output "expected      : 0068 1ED3 006E 0067  (h o-circumflex-grave n g)"

    if ($text -eq ("h" + [char]0x1ED3 + "ng")) {
        Write-Output "RESULT: PASS - the address-bar guard works"
    } elseif ($text -like "ho*") {
        Write-Output "RESULT: FAIL - still the leading-o bug (hoong shape)"
    } else {
        Write-Output "RESULT: FAIL - unexpected text"
    }
}
finally {
    # Escape restores the address bar to the URL it held. Nothing is committed:
    # no Enter is sent on any path, and the window is the user's, so it is
    # emphatically not closed.
    try { [K]::Tap([uint16]$VK_ESCAPE) } catch {}
}
