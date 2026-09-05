# A plain Win32 edit control in its own process, as a verification target.
#
# Windows 11 replaced Notepad with a WinUI application whose text surface is not
# a window at all, so `FindWindowEx(pad, "Edit")` returns nothing and every
# harness that reads its result back through `WM_GETTEXT` is blind. That is a
# limitation of the target, not of GlowKey — but it makes the Tier 1 smoke test
# unrunnable on a current machine, which is worse than it sounds: Tier 1 is what
# proves the hook transforms anything at all.
#
# This is a target that stays readable: a WinForms `TextBox` is a subclassed
# Win32 EDIT, so it answers `WM_SETTEXT`/`WM_GETTEXT` exactly as the old Notepad
# did, and it runs in its own process, so the keystroke path under test is still
# the cross-process one.
#
# It writes the control's window handle to `-HandlePath` once shown, because the
# class name WinForms gives it (`WindowsForms10.EDIT.app.0.…`) is generated and
# cannot be searched for reliably. The harness waits for that file.
#
# Not a substitute for verifying in real applications — a single-window Win32
# edit is the friendliest possible host. Tier 2 is where the difficult ones live.

param(
    [Parameter(Mandatory = $true)]
    [string]$HandlePath
)

$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$form = New-Object System.Windows.Forms.Form
$form.Text = 'GlowKey verification target'
$form.Width = 700
$form.Height = 240
$form.StartPosition = 'CenterScreen'

$box = New-Object System.Windows.Forms.TextBox
$box.Multiline = $true
$box.Dock = 'Fill'
$box.Font = New-Object System.Drawing.Font('Consolas', 14)
$form.Controls.Add($box)

$form.Add_Shown({
        $box.Focus()
        # The handle is only valid once the control has been created, which is
        # why this is in Shown rather than before Run.
        Set-Content -Path $HandlePath -Value ([int64]$box.Handle) -Encoding ASCII
    })

[System.Windows.Forms.Application]::Run($form)
