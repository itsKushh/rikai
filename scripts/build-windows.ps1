# Depuis une Developer PowerShell x64 (Visual Studio Build Tools, SDK Windows, CMake, uv et Inno Setup 6).
param(
    [ValidateSet('All','Dependencies','PythonTests','RustTests','Binaries','Worker','Installer')]
    [string]$Stage = 'All'
)
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
function Run-Checked([string]$Program,[string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program a échoué (code $LASTEXITCODE)." }
}
if (-not [Environment]::Is64BitProcess -or $env:OS -ne 'Windows_NT') { throw 'Windows x64 requis.' }
if (Test-Path Env:GPUI_RENDER_ALLOW_MISSING_DXBC) { throw 'Les shaders DXBC sont obligatoires pour la distribution.' }
$Python = Join-Path $PWD 'ai-worker/.venv/Scripts/python.exe'
if ($Stage -in 'All','Dependencies') {
    Run-Checked uv @('sync','--directory','ai-worker','--locked','--python','3.12','--extra','summary','--extra','packaging')
}
if ($Stage -in 'All','PythonTests') {
    Run-Checked $Python @('-X','faulthandler','-u','-m','unittest','discover','-s','ai-worker/tests','-v')
}
if ($Stage -in 'All','RustTests') {
    Run-Checked cargo @('test','--locked','--release','--workspace')
}
if ($Stage -in 'All','Binaries') {
    Run-Checked cargo @('build','--locked','--release','--workspace')
}
if ($Stage -in 'All','Worker') {
    Run-Checked $Python @('-m','PyInstaller','--noconfirm','--clean','--onedir','--name','rikai-worker','--collect-all','sherpa_onnx','--copy-metadata','sherpa-onnx-core','--collect-all','onnxruntime','--collect-all','ctranslate2','--collect-all','faster_whisper','--collect-all','llama_cpp','--collect-all','sentencepiece','--collect-data','py3langid','--copy-metadata','tqdm','--copy-metadata','huggingface-hub','ai-worker/worker.py')
    Run-Checked $Python @('scripts/test-worker-bundle.py','dist/rikai-worker/rikai-worker.exe')
}
if ($Stage -in 'All','Installer') {
    Run-Checked $Python @('scripts/notices.py')
    $Compiler = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($Compiler) { $IsccPath = $Compiler.Source }
    else { $IsccPath = Join-Path "${env:ProgramFiles(x86)}" 'Inno Setup 6\ISCC.exe' }
    $Version = (Select-String -Path Cargo.toml -Pattern '^version = "([^"]+)"').Matches[0].Groups[1].Value
    Run-Checked $IsccPath @("/DRikaiVersion=$Version", 'packaging/Rikai.iss')
}
