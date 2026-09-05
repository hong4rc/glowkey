# Run a verification harness on its own Windows desktop.
#
# A Windows *desktop* (`CreateDesktop`, not a virtual desktop) has its own input
# queue, its own foreground window and its own hooks. Anything typed there cannot
# reach the desktop the user is looking at, and the user's typing cannot reach the
# test. That is the difference between "please don't touch the machine for thirty
# seconds" and a test that can run whenever.
#
# It also makes the result cleaner: nothing the user happens to be doing can
# perturb it, and no other input method on the live desktop can contaminate it —
# which is what happened the first time these tests ran alongside EVKey.
#
# The desktop assignment happens through `STARTUPINFO.lpDesktop`, which is the
# only way to do it. .NET's `ProcessStartInfo` cannot express it, so this
# P/Invokes `CreateProcessW` rather than pretending.
#
# Usage:
#   .\scripts\verify-windows-isolated.ps1 .\scripts\verify-windows-tier1.ps1
#
# Nothing appears on the user's screen. GlowKey is started on the isolated
# desktop, the harness runs there, and both are torn down afterwards.

param(
    [Parameter(Mandatory = $true)]
    [string]$Harness
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path $Harness)) { throw "harness not found: $Harness" }
$harnessFull = (Resolve-Path $Harness).Path
$repo = (Get-Location).Path

Add-Type @'
using System;
using System.Runtime.InteropServices;

public class Iso {
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
    public struct STARTUPINFO {
        public int cb; public string lpReserved; public string lpDesktop; public string lpTitle;
        public int dwX, dwY, dwXSize, dwYSize, dwXCountChars, dwYCountChars, dwFillAttribute;
        public int dwFlags; public short wShowWindow; public short cbReserved2;
        public IntPtr lpReserved2, hStdInput, hStdOutput, hStdError;
    }
    [StructLayout(LayoutKind.Sequential)]
    public struct PROCESS_INFORMATION { public IntPtr hProcess, hThread; public int dwProcessId, dwThreadId; }

    [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    public static extern IntPtr CreateDesktopW(string name, IntPtr dev, IntPtr dm, uint flags, uint access, IntPtr sa);
    [DllImport("user32.dll", SetLastError=true)] public static extern bool CloseDesktop(IntPtr h);

    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    public static extern bool CreateProcessW(string app, string cmd, IntPtr pa, IntPtr ta,
        bool inherit, uint flags, IntPtr env, string cwd, ref STARTUPINFO si, out PROCESS_INFORMATION pi);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern uint WaitForSingleObject(IntPtr h, uint ms);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool TerminateProcess(IntPtr h, uint code);
    [DllImport("kernel32.dll", SetLastError=true)] public static extern bool CloseHandle(IntPtr h);

    public const uint DESKTOP_ALL = 0x10000000; // GENERIC_ALL
    public const uint CREATE_NO_WINDOW = 0x08000000;

    /// Launches a command on `desktop` and returns its process handle.
    public static PROCESS_INFORMATION StartOn(string desktop, string commandLine, string cwd) {
        STARTUPINFO si = new STARTUPINFO();
        si.cb = Marshal.SizeOf(typeof(STARTUPINFO));
        si.lpDesktop = desktop;          // the whole point
        PROCESS_INFORMATION pi;
        if (!CreateProcessW(null, commandLine, IntPtr.Zero, IntPtr.Zero, false,
                            CREATE_NO_WINDOW, IntPtr.Zero, cwd, ref si, out pi)) {
            throw new Exception("CreateProcessW failed: " + Marshal.GetLastWin32Error());
        }
        return pi;
    }
}
'@

$deskName = "GlowKeyTest-" + [Guid]::NewGuid().ToString("N").Substring(0, 8)
$desk = [Iso]::CreateDesktopW($deskName, [IntPtr]::Zero, [IntPtr]::Zero, 0, [Iso]::DESKTOP_ALL, [IntPtr]::Zero)
if ($desk -eq [IntPtr]::Zero) {
    throw "could not create an isolated desktop (error $([Runtime.InteropServices.Marshal]::GetLastWin32Error()))"
}
Write-Output "Isolated desktop: $deskName  (nothing below touches your session)"

$resultFile = Join-Path $env:TEMP "glowkey-isolated-$deskName.txt"
$runnerFile = Join-Path $env:TEMP "glowkey-isolated-$deskName.ps1"

# The harness writes to a file rather than stdout: a process on another desktop
# has no console to inherit from this one.
@"
Set-Location '$repo'
try { & '$harnessFull' *>&1 | Out-File -FilePath '$resultFile' -Encoding UTF8 }
catch { `$_ | Out-File -FilePath '$resultFile' -Encoding UTF8 }
"@ | Set-Content -Path $runnerFile -Encoding UTF8

$glow = $null
$test = $null
try {
    # GlowKey on the isolated desktop, so its hook and injection live there too.
    $glow = [Iso]::StartOn($deskName, "`"$repo\target\release\GlowKey.exe`"", $repo)
    Start-Sleep -Seconds 3

    $test = [Iso]::StartOn($deskName,
        "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$runnerFile`"", $repo)

    # 3 minutes is generous for any Tier 1/2 harness; a hang is a result too.
    $waited = [Iso]::WaitForSingleObject($test.hProcess, 180000)
    if ($waited -ne 0) {
        Write-Warning "harness did not finish within 180s — terminating"
        [void][Iso]::TerminateProcess($test.hProcess, 1)
    }

    Write-Output ""
    if (Test-Path $resultFile) { Get-Content $resultFile } else { Write-Output "(no output)" }
}
finally {
    foreach ($p in @($test, $glow)) {
        if ($p -and $p.hProcess -ne [IntPtr]::Zero) {
            [void][Iso]::TerminateProcess($p.hProcess, 0)
            [void][Iso]::CloseHandle($p.hProcess)
            [void][Iso]::CloseHandle($p.hThread)
        }
    }
    Remove-Item $runnerFile, $resultFile -ErrorAction SilentlyContinue
    [void][Iso]::CloseDesktop($desk)
}
