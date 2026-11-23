$ErrorActionPreference = 'Stop'
$Version = (Select-String -Path Cargo.toml -Pattern '^version = "([^"]+)"').Matches[0].Groups[1].Value
$Destination = Join-Path $env:RUNNER_TEMP "Rikai-install-$Version"
$Installer = (Resolve-Path 'dist/Rikai-Setup-x64.exe').Path
$Result = Start-Process $Installer -Wait -PassThru -ArgumentList @(
    '/SP-', '/VERYSILENT', '/NORESTART', '/SUPPRESSMSGBOXES', "/DIR=`"$Destination`""
)
if ($Result.ExitCode -ne 0) { throw "Installation interrompue ($($Result.ExitCode))." }
foreach ($Name in 'rikai.exe','rikai-audio.exe','worker/rikai-worker.exe','rikai.ico') {
    if (-not (Test-Path (Join-Path $Destination $Name) -PathType Leaf)) { throw "Composant absent : $Name" }
}
$Info = (Get-Item (Join-Path $Destination 'rikai.exe')).VersionInfo
if ($Info.ProductName -ne 'Rikai' -or $Info.ProductVersion -ne $Version) {
    throw "Métadonnées de version inattendues : $($Info.ProductName) $($Info.ProductVersion)"
}
Write-Output "Rikai $Version : installation utilisateur et ressources de version vérifiées."
