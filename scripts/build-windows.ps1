# Depuis une Developer PowerShell x64 (Visual Studio Build Tools, SDK Windows, CMake, uv et Inno Setup 6).
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
function Run-Checked([string]$Program,[string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program a échoué (code $LASTEXITCODE)." }
}
if (-not [Environment]::Is64BitProcess -or $env:OS -ne 'Windows_NT') { throw 'Windows x64 requis.' }
if (Test-Path Env:GPUI_RENDER_ALLOW_MISSING_DXBC) { throw 'Les shaders DXBC sont obligatoires pour la distribution.' }
Push-Location ai-worker
try {
    Run-Checked uv @('sync','--locked','--python','3.12','--extra','summary','--extra','packaging')
    $Python = (Resolve-Path '.venv/Scripts/python.exe').Path
    Run-Checked $Python @('-m','unittest','discover','-s','tests','-v')
} finally { Pop-Location }
Run-Checked cargo @('test','--locked','--workspace')
Run-Checked cargo @('build','--locked','--release','--workspace')
Run-Checked $Python @('-m','PyInstaller','--noconfirm','--clean','--onedir','--name','meetingnotes-worker','--collect-all','sherpa_onnx','--copy-metadata','sherpa-onnx-core','--collect-all','onnxruntime','--collect-all','ctranslate2','--collect-all','faster_whisper','--collect-all','llama_cpp','--copy-metadata','tqdm','--copy-metadata','huggingface-hub','ai-worker/worker.py')
Run-Checked $Python @('scripts/test-worker-bundle.py','dist/meetingnotes-worker/meetingnotes-worker.exe')
Run-Checked $Python @('scripts/notices.py')
$Compiler = Get-Command ISCC.exe -ErrorAction SilentlyContinue
if ($Compiler) { $IsccPath = $Compiler.Source }
else { $IsccPath = Join-Path "${env:ProgramFiles(x86)}" 'Inno Setup 6\ISCC.exe' }
Run-Checked $IsccPath @('packaging/MeetingNotes.iss')

