$ErrorActionPreference='Stop'
$modelDir=Join-Path $env:LOCALAPPDATA 'Local Chess Coach/models'
New-Item -ItemType Directory -Force -Path $modelDir | Out-Null
$model=Join-Path $modelDir 'Qwen3-4B-Instruct-2507-Q4_K_M.gguf'
$expected='3605803b982cb64aead44f6c1b2ae36e3acdb41d8e46c8a94c6533bc4c67e597'
if(-not(Test-Path -LiteralPath $model)){
 $partial=Join-Path $modelDir 'Qwen3-4B-Instruct-2507-Q4_K_M.partial'
 Invoke-WebRequest -Uri 'https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/a06e946bb6b655725eafa393f4a9745d460374c9/Qwen3-4B-Instruct-2507-Q4_K_M.gguf' -OutFile $partial
 if((Get-FileHash -LiteralPath $partial).Hash.ToLowerInvariant() -ne $expected){throw 'Model checksum mismatch; original model preserved'}
 Move-Item -LiteralPath $partial -Destination $model
}
if((Get-FileHash -LiteralPath $model).Hash.ToLowerInvariant() -ne $expected){throw 'Model checksum mismatch'}
Write-Host 'Qwen local AI ready. Choose Local AI and server managed in Coach settings. No cloud key required.'
