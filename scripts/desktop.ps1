param(
    [ValidateSet('Inspect','Capture','Click','Close','Tray','Exit','Move','Lifecycle')][string]$Action = 'Inspect',
    [int]$ProcessId,
    [int]$X = 0,
    [int]$Y = 0,
    [string]$OutputPath = ''
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
using System.Collections.Generic;
public static class PickerDesktop {
    public delegate bool EnumProc(IntPtr hwnd, IntPtr param);
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct TrayIdentifier { public uint Size; public IntPtr Hwnd; public uint Id; public Guid Guid; }
    [DllImport("shell32.dll")] public static extern int Shell_NotifyIconGetRect(ref TrayIdentifier id, out Rect rect);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc fn, IntPtr param);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int count);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hwnd, uint msg, IntPtr wp, IntPtr lp);
    [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr menu);
    [DllImport("user32.dll")] public static extern uint GetMenuItemID(IntPtr menu, int position);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetMenuString(IntPtr menu, uint item, StringBuilder text, int count, uint flags);
    [DllImport("user32.dll")] public static extern bool GetMenuItemRect(IntPtr hwnd, IntPtr menu, uint item, out Rect rect);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wp, IntPtr lp);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hwnd, int x, int y, int width, int height, bool repaint);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    public static IntPtr[] Windows(uint processId) {
        var result = new List<IntPtr>();
        EnumWindows((hwnd, param) => { uint pid; GetWindowThreadProcessId(hwnd, out pid); if (pid == processId) result.Add(hwnd); return true; }, IntPtr.Zero);
        return result.ToArray();
    }
    public static bool TrayRect(IntPtr hwnd, out Rect rect) {
        rect = new Rect();
        for(uint i=0;i<8;i++) { var id=new TrayIdentifier {Size=(uint)Marshal.SizeOf<TrayIdentifier>(),Hwnd=hwnd,Id=i}; if(Shell_NotifyIconGetRect(ref id,out rect)==0) return true; }
        return false;
    }
    public static IntPtr Popup(uint processId) {
        foreach(var window in Windows(processId)) { var name=new StringBuilder(256); GetClassName(window,name,256); if(name.ToString()=="#32768" && IsWindowVisible(window)) return window; }
        return IntPtr.Zero;
    }
}
'@
[PickerDesktop]::SetProcessDPIAware() | Out-Null
$rows = @([PickerDesktop]::Windows($ProcessId) | ForEach-Object {
    $title = [System.Text.StringBuilder]::new(512)
    $class = [System.Text.StringBuilder]::new(256)
    [PickerDesktop]::GetWindowText($_, $title, 512) | Out-Null
    [PickerDesktop]::GetClassName($_, $class, 256) | Out-Null
    $rect = [PickerDesktop+Rect]::new()
    [PickerDesktop]::GetWindowRect($_, [ref]$rect) | Out-Null
    [PSCustomObject]@{ Handle=$_; Title=$title.ToString(); Class=$class.ToString(); Visible=[PickerDesktop]::IsWindowVisible($_); Left=$rect.Left; Top=$rect.Top; Width=$rect.Right-$rect.Left; Height=$rect.Bottom-$rect.Top }
})
$main = $rows | Where-Object Title -eq 'AI Picker' | Select-Object -First 1
$tray = $rows | Where-Object Class -eq 'tray_icon_app' | Select-Object -First 1
if ($Action -eq 'Inspect') {
    $rows | ConvertTo-Json
    if ($tray) { $trayRect=[PickerDesktop+Rect]::new(); "Tray registered: $([PickerDesktop]::TrayRect($tray.Handle,[ref]$trayRect))" }
    exit
}
if (!$main) { throw "AI Picker main window not found for process $ProcessId" }
switch ($Action) {
    'Capture' {
        if (!$OutputPath) { throw 'OutputPath required' }
        [PickerDesktop]::SetForegroundWindow($main.Handle) | Out-Null
        Start-Sleep -Milliseconds 300
        $bitmap = [System.Drawing.Bitmap]::new($main.Width, $main.Height)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen($main.Left, $main.Top, 0, 0, $bitmap.Size)
            $bitmap.Save($OutputPath, [System.Drawing.Imaging.ImageFormat]::Png)
        } finally { $graphics.Dispose(); $bitmap.Dispose() }
        Write-Output $OutputPath
    }
    'Click' {
        [PickerDesktop]::SetForegroundWindow($main.Handle) | Out-Null
        $point = [PickerDesktop+Point]::new(); $point.X=$X; $point.Y=$Y
        [PickerDesktop]::ClientToScreen($main.Handle, [ref]$point) | Out-Null
        [PickerDesktop]::SetCursorPos($point.X, $point.Y) | Out-Null
        Start-Sleep -Milliseconds 100
        [PickerDesktop]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
        [PickerDesktop]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
    }
    'Close' { [PickerDesktop]::PostMessage($main.Handle, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }
    'Tray' {
        if (!$tray) { throw 'Tray window missing' }
        # Deliver the same native tray notification used by Explorer on left release.
        [PickerDesktop]::PostMessage($tray.Handle, 6002, [IntPtr]::Zero, [IntPtr]0x202) | Out-Null
    }
    'Lifecycle' {
        if (!$tray) { throw 'Tray window missing' }
        $trayRect=[PickerDesktop+Rect]::new()
        if (![PickerDesktop]::TrayRect($tray.Handle,[ref]$trayRect)) { throw 'Shell does not contain the tray icon' }
        [PickerDesktop]::PostMessage($main.Handle,0x10,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
        $deadline=[DateTime]::UtcNow.AddSeconds(5)
        while ([PickerDesktop]::IsWindowVisible($main.Handle) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
        if ([PickerDesktop]::IsWindowVisible($main.Handle)) { throw 'Close did not hide the window' }
        if (!(Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)) { throw 'Close killed the process' }
        [PickerDesktop]::PostMessage($tray.Handle,6002,[IntPtr]::Zero,[IntPtr]0x202) | Out-Null
        $deadline=[DateTime]::UtcNow.AddSeconds(5)
        while (![PickerDesktop]::IsWindowVisible($main.Handle) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
        if (![PickerDesktop]::IsWindowVisible($main.Handle)) { throw 'Tray click did not restore the window' }
        'PASS: shell icon registered; close hides; process survives; tray click restores.'
    }
    'Exit' {
        if (!$tray) { throw 'Tray window missing' }
        [PickerDesktop]::SetCursorPos(600, 400) | Out-Null
        [PickerDesktop]::PostMessage($tray.Handle, 6002, [IntPtr]::Zero, [IntPtr]0x205) | Out-Null
        $deadline=[DateTime]::UtcNow.AddSeconds(5)
        $popup=[IntPtr]::Zero
        while ($popup -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100; $popup=[PickerDesktop]::Popup($ProcessId) }
        if ($popup -eq [IntPtr]::Zero) { throw 'Tray menu did not open' }
        $menu=[PickerDesktop]::SendMessage($popup,0x1E1,[IntPtr]::Zero,[IntPtr]::Zero)
        $count=[PickerDesktop]::GetMenuItemCount($menu)
        $exitFound=$false
        for($item=0;$item -lt $count;$item++) {
            $label=[System.Text.StringBuilder]::new(256)
            [PickerDesktop]::GetMenuString($menu,$item,$label,256,0x400) | Out-Null
            if($label.ToString() -eq 'Выход') {
                # muda attaches the menu command handler to this exact tray HWND.
                # Cancel tracking and dispatch the discovered Exit ID to that handler.
                $commandId=[PickerDesktop]::GetMenuItemID($menu,$item)
                if ($commandId -eq [uint32]::MaxValue) { throw 'Exit menu command ID missing' }
                [PickerDesktop]::PostMessage($tray.Handle,0x1F,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
                [PickerDesktop]::PostMessage($tray.Handle,0x111,[IntPtr]$commandId,[IntPtr]::Zero) | Out-Null
                $exitFound=$true
                break
            }
        }
        if(!$exitFound) { throw 'Exit menu item missing' }
    }
    'Move' { [PickerDesktop]::MoveWindow($main.Handle, $X, $Y, $main.Width, $main.Height, $true) | Out-Null }
}
