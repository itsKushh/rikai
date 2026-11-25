# Windows PowerShell (STA, .NET Framework), sharing the WPF dispatch loop with the actual window.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase
$assembly = [Reflection.Assembly]::LoadFrom((Resolve-Path dist/Rikai-Setup-x64.exe))
[Windows.Application]::ResourceAssembly = $assembly
$app = New-Object Windows.Application
$app.ShutdownMode = 'OnExplicitShutdown'
[Threading.Thread]::CurrentThread.CurrentUICulture = [Globalization.CultureInfo]::GetCultureInfo('fr-FR')
$assembly.GetType('Rikai.Setup.Program').GetMethod('LoadStrings', [Reflection.BindingFlags]'NonPublic,Static').Invoke($null, @( $app ))
$argsForWindow = New-Object 'object[]' 1
$argsForWindow[0] = [string[]]@()
$window = [Activator]::CreateInstance($assembly.GetType('Rikai.Setup.SetupWindow'), $argsForWindow)
$folder = Join-Path $env:RUNNER_TEMP 'Rikai guided install'
$preview = New-Item -Force -ItemType Directory dist/installer-previews
function Tick {
    $window.Dispatcher.Invoke([Action]{}, [Windows.Threading.DispatcherPriority]::Background)
    Start-Sleep -Milliseconds 35
}
function Click([string]$id) {
    $button = $window.FindName($id)
    if (-not $button.IsEnabled -or -not $button.IsVisible) { throw "Unavailable button $id" }
    $button.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Button]::ClickEvent))
}
function Snapshot([string]$name) {
    $window.UpdateLayout()
    $image = [Windows.Media.Imaging.RenderTargetBitmap]::new([int]$window.ActualWidth, [int]$window.ActualHeight, 96, 96, [Windows.Media.PixelFormats]::Pbgra32)
    $image.Render($window)
    $encoder = New-Object Windows.Media.Imaging.PngBitmapEncoder
    $encoder.Frames.Add([Windows.Media.Imaging.BitmapFrame]::Create($image))
    $stream = [IO.File]::Create((Join-Path $preview.FullName "$name.png"))
    try { $encoder.Save($stream) } finally { $stream.Dispose() }
}
function Await-Result([bool]$cancel) {
    $observed = $false
    $deadline = [DateTime]::UtcNow.AddMinutes(4)
    while (-not $window.FindName('CloseButton').IsVisible) {
        Tick
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Timed out while installing' }
        $progress = [int]($window.FindName('Percent').Text -replace '[^0-9]', '')
        if (!$observed -and $progress -gt 1 -and $progress -lt 99) {
            $observed = $true
            if ($cancel) { Click 'MainButton' } else { Snapshot 'guided' }
        }
    }
    if (!$observed) { throw 'No intermediate progress received from the installer' }
    $desired = if ($cancel) {'Cancelled'} else {'Done'}
    if ($window.FindName('Heading').Text -ne $app.Resources[$desired]) {
        $latest = Get-ChildItem "$env:TEMP/Rikai-Setup-*.log" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if ($latest) { Get-Content -Tail 35 $latest.FullName }
        throw "Unexpected outcome: $($window.FindName('Heading').Text) / $($window.FindName('Status').Text)"
    }
}
try {
    $window.Show()
    Tick
    Snapshot 'folder'
    $window.FindName('Destination').Text = 'relative'
    Click 'InstallButton'
    if ($window.FindName('PathHint').Text -ne $app.Resources['Invalid']) { throw 'Relative path accepted' }
    $window.FindName('Destination').Text = $folder
    Click 'InstallButton'
    Await-Result $true
    if (Test-Path "$folder/rikai.exe") { throw 'Cancelled installation was not rolled back' }
    Click 'MainButton'
    Click 'InstallButton'
    Await-Result $false
    if (!(Test-Path "$folder/worker/rikai-worker.exe")) { throw 'Worker missing at the selected location' }
    Snapshot 'complete'
    Click 'CloseButton'
} finally { $app.Shutdown() }
