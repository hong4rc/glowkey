# Capture the UI Automation identity of whatever you focus.
#
# The Chromium address bar needs a forward-delete before an edit (its inline
# autocomplete keeps a trailing selection that eats the first Backspace); a text
# box in a page must never get one, because there the forward-delete deletes a
# real character. Telling the two apart is the whole of the fix in phase 3.1 of
# `plans/260905-1643-glowkey-production-hardening/`, and it needs a stable
# property to test — one that is not the element's *name*, which is localized and
# changes between Chrome and Edge and between releases.
#
# So: run this, focus the things below in turn, and it prints what UIA says about
# each. The output decides the predicate; nothing else about the fix is unknown.
#
#   .\scripts\probe-omnibox-identity.ps1
#
# Then focus, giving each a second:
#   1. the Edge/Chrome ADDRESS BAR          <- must be guarded
#   2. a text box in a PAGE (Gmail, a search box)  <- must NOT be guarded
#   3. Notepad or any ordinary Win32 edit    <- must NOT be guarded
#
# Ctrl+C when done, and paste the output back.
#
# Read-only: it observes focus and types nothing. Safe to run while you work.
#
# Windows PowerShell 5.1 is the reliable host for the UIA assemblies. If you are
# in PowerShell 7 and it cannot load them, run:
#   powershell -ExecutionPolicy Bypass -File .\scripts\probe-omnibox-identity.ps1

$ErrorActionPreference = 'Stop'

try {
    Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
} catch {
    Write-Output "Could not load the UI Automation assemblies in this host."
    Write-Output "Re-run under Windows PowerShell 5.1:"
    Write-Output "  powershell -ExecutionPolicy Bypass -File .\scripts\probe-omnibox-identity.ps1"
    exit 1
}

function Describe($element, $label) {
    if ($null -eq $element) { return }
    $c = $element.Current
    Write-Output ""
    Write-Output "── $label ─────────────────────────────────────────────"
    Write-Output ("  ControlType   : {0}" -f $c.ControlType.ProgrammaticName)
    Write-Output ("  LocalizedType : {0}" -f $c.LocalizedControlType)
    Write-Output ("  Name          : {0}" -f $c.Name)
    Write-Output ("  AutomationId  : {0}" -f $c.AutomationId)
    Write-Output ("  ClassName     : {0}" -f $c.ClassName)
    Write-Output ("  FrameworkId   : {0}" -f $c.FrameworkId)
    Write-Output ("  IsPassword    : {0}" -f $c.IsPassword)
    Write-Output ("  NativeHwnd    : 0x{0:X}" -f $c.NativeWindowHandle)
    Write-Output ("  BoundingRect  : {0}" -f $c.BoundingRectangle)

    # Whether the element exposes a text selection at all. The address bar does;
    # this is the property the macOS guard keys on.
    $tp = $null
    if ($element.TryGetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern, [ref]$tp)) {
        try {
            $sel = $tp.GetSelection()
            $len = if ($sel -and $sel.Count -gt 0) { $sel[0].GetText(200).Length } else { 0 }
            Write-Output ("  TextPattern   : yes, selection length {0}" -f $len)
        } catch {
            Write-Output "  TextPattern   : yes, selection unreadable"
        }
    } else {
        Write-Output "  TextPattern   : no"
    }

    # The ancestor chain is what separates an address bar from a page text box
    # when the leaf looks the same: the omnibox sits under the browser's toolbar.
    try {
        $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
        $node = $walker.GetParent($element)
        $depth = 0
        $chain = @()
        while ($null -ne $node -and $depth -lt 6) {
            $nc = $node.Current
            $chain += ("{0}[{1}]" -f $nc.ControlType.ProgrammaticName.Replace('ControlType.', ''), $nc.Name)
            $node = $walker.GetParent($node)
            $depth++
        }
        Write-Output ("  Ancestors     : {0}" -f ($chain -join ' < '))
    } catch {
        Write-Output "  Ancestors     : unavailable"
    }
}

Write-Output "Watching focus. Focus the address bar, then a page text box, then Notepad."
Write-Output "Ctrl+C to stop. Nothing is typed; this only observes."

$last = ""
while ($true) {
    try {
        $el = [System.Windows.Automation.AutomationElement]::FocusedElement
        if ($null -ne $el) {
            $c = $el.Current
            # A crude identity so the same field is not printed on every tick.
            $key = "{0}|{1}|{2}|0x{3:X}" -f $c.ControlType.ProgrammaticName, $c.AutomationId, $c.ClassName, $c.NativeWindowHandle
            if ($key -ne $last) {
                $last = $key
                $proc = try { (Get-Process -Id $c.ProcessId -ErrorAction Stop).ProcessName } catch { "?" }
                Describe $el "focus in $proc"
            }
        }
    } catch {
        # Focus can vanish mid-read; that is normal, keep watching.
    }
    Start-Sleep -Milliseconds 400
}
