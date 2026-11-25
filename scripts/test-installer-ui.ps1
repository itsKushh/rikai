$ErrorActionPreference = 'Stop'
$Framework = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319'
$Driver = Join-Path $env:RUNNER_TEMP 'TestInstallerWindow.exe'
$RefDir = Join-Path "${env:ProgramFiles(x86)}" 'Reference Assemblies\Microsoft\Framework\.NETFramework\v4.8'
$References = 'System.Xaml','WindowsBase','PresentationCore','PresentationFramework' | ForEach-Object { "/reference:$RefDir\$_.dll" }
& "$Framework/csc.exe" /nologo /target:exe /platform:x64 "/out:$Driver" @References (Join-Path $PSScriptRoot 'TestInstallerWindow.cs')
if ($LASTEXITCODE -ne 0) { throw 'Could not compile the installer UI test' }
& $Driver (Resolve-Path dist/Rikai-Setup-x64.exe).Path (Join-Path $env:RUNNER_TEMP 'Rikai guided install') (Join-Path $PWD 'dist/installer-previews')
if ($LASTEXITCODE -ne 0) {
    $Log = Get-ChildItem "$env:TEMP/Rikai-Setup-*.log" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($Log) { Get-Content -Tail 24 $Log.FullName }
    throw 'Installer UI test failed'
}
