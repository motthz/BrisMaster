param([string]$Folder)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition 'using System.Runtime.InteropServices; public static class ReplayDpi { [DllImport("user32.dll")] public static extern System.IntPtr SetThreadDpiAwarenessContext(System.IntPtr context); }'
[void][ReplayDpi]::SetThreadDpiAwarenessContext([IntPtr]::new(-4))
$form = New-Object Windows.Forms.Form
$form.FormBorderStyle = 'None'
$form.TopMost = $true
$form.StartPosition = 'Manual'
$form.AutoScaleMode = 'None'
$form.BackgroundImageLayout = 'Stretch'
$form.BackgroundImage = [Drawing.Image]::FromFile((Join-Path $Folder 'table.png'))
$primary = [Windows.Forms.Screen]::PrimaryScreen
$other = [Windows.Forms.Screen]::AllScreens | Where-Object { $_.DeviceName -ne $primary.DeviceName } | Select-Object -First 1
$timer = New-Object Windows.Forms.Timer
$timer.Interval = 80
$script:phase = 0
$timer.Add_Tick({
    $next = $script:phase + 1
    if (!(Test-Path (Join-Path $Folder "go-$next"))) { return }
    $script:phase = $next
    if ($next -eq 5) { $form.Close(); return }
    if ($next -eq 3) {
        $form.WindowState = 'Minimized'
    } else {
        $form.WindowState = 'Normal'
        $w = if ($next -eq 1) { 700 } else { 350 }
        $h = if ($next -eq 1) { 450 } else { 225 }
        $screen = if ($other -and $next -eq 2) { $other.WorkingArea } else { $primary.WorkingArea }
        $form.Size = [Drawing.Size]::new($w, $h)
        $form.Location = [Drawing.Point]::new($screen.X + 16 * $next, $screen.Y + 18 * $next)
        $form.Activate()
        $form.Refresh()
    }
    [IO.File]::WriteAllText((Join-Path $Folder "ready-$next"), 'ready')
})
try {
    $timer.Start()
    [Windows.Forms.Application]::Run($form)
} finally {
    $timer.Dispose()
    $form.BackgroundImage.Dispose()
    $form.Dispose()
}
