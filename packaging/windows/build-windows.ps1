# Builds Sheshnag for Windows: a setup .exe (Inno Setup) and a portable .zip.
# Run from PowerShell on Windows. Needs: Rust toolchain and Inno Setup (choco install innosetup -y).
$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $Root
$Version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches.Groups[1].Value
$Dist = Join-Path $Root "dist"
New-Item -ItemType Directory -Force -Path $Dist | Out-Null

cargo build --release --locked
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

# Portable zip
$Stage = Join-Path $Root "target\windows-package\Sheshnag"
Remove-Item -Recurse -Force (Split-Path $Stage) -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Copy-Item "target\release\sheshnag-ui.exe" (Join-Path $Stage "sheshnag.exe")
Copy-Item "README.md" $Stage
Compress-Archive -Path $Stage -DestinationPath (Join-Path $Dist "Sheshnag-$Version-windows-portable.zip") -Force

# Installer
$Iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
if (-not $Iscc) { $Iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" }
& $Iscc "/DAppVersion=$Version" (Join-Path $PSScriptRoot "sheshnag.iss")
if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed" }

Get-ChildItem $Dist -Filter "Sheshnag-$Version-windows*" | ForEach-Object {
  $h = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
  "$h  $($_.Name)" | Out-File -Append -Encoding ascii (Join-Path $Dist "SHA256SUMS-windows.txt")
}
Write-Host "Done. Artifacts in $Dist"
