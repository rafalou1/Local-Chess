param([string]$Archive='')
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
$manifest=Get-Content -LiteralPath (Join-Path $root 'build-resources.json') -Raw | ConvertFrom-Json
if(!$Archive){
 $cache=Join-Path $root '.cache'
 New-Item -ItemType Directory -Force $cache | Out-Null
 $Archive=Join-Path $cache "build-resources-$($manifest.version).zip"
 if(!(Test-Path -LiteralPath $Archive)){
  Write-Output 'Downloading the pinned Windows build resources...'
  Invoke-WebRequest -Uri $manifest.url -OutFile $Archive
 }
}
if((Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash -ine $manifest.sha256){throw 'Resource checksum mismatch. Remove the cached ZIP and download it again.'}
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip=[IO.Compression.ZipFile]::OpenRead([IO.Path]::GetFullPath($Archive))
try{
 $prefix=[IO.Path]::GetFullPath($root).TrimEnd('\')+'\'
 foreach($entry in $zip.Entries){
  $name=$entry.FullName.Replace('\','/')
  if(!($name.StartsWith('src-tauri/resources/') -or $name.StartsWith('third-party/'))){throw 'Unexpected resource archive path'}
  $target=[IO.Path]::GetFullPath((Join-Path $root $name))
  if(!$target.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)){throw 'Resource archive path escapes the project'}
 }
}finally{$zip.Dispose()}
Expand-Archive -LiteralPath $Archive -DestinationPath $root -Force
Write-Output 'Build resources verified and ready.'
