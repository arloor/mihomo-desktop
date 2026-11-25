param(
    [string]$CorePath = "",
    [string]$GitHubMirror = "",
    [string]$Version = "",
    [switch]$DownloadCore,
    [switch]$SkipBuild,
    [switch]$RefreshGeo
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

if (-not $Version) {
    $cargoToml = Get-Content (Join-Path $RepoRoot "Cargo.toml") -Raw
    if ($cargoToml -match '(?m)^version\s*=\s*"([^"]+)"') {
        $Version = $Matches[1]
    } else {
        $Version = "0.1.0"
    }
}
$Version = $Version.TrimStart('v')
Write-Host "==> Target version: $Version" -ForegroundColor Cyan

$MirrorPrefix = if ($GitHubMirror) { $GitHubMirror.TrimEnd('/') + '/' } else { "" }

Write-Host "==> 1. Building mihomo-desktop (release)..." -ForegroundColor Cyan
if (-not $SkipBuild) {
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build --release failed"
    }
}

$DesktopExe = Join-Path $RepoRoot "target\release\mihomo-desktop.exe"
if (-not (Test-Path $DesktopExe)) {
    throw "Release binary not found: $DesktopExe"
}

Write-Host "==> 2. Locating bundled mihomo.exe..." -ForegroundColor Cyan
$Candidates = @()
if ($CorePath) { $Candidates += $CorePath }
$Candidates += @(
    (Join-Path $RepoRoot "installer\mihomo.exe"),
    "C:\Program Files\Mihomo\mihomo.exe"
)

$ResolvedCore = $null
foreach ($c in $Candidates) {
    if ($c -and (Test-Path $c)) {
        $ResolvedCore = (Resolve-Path $c).Path
        break
    }
}

if (-not $ResolvedCore -and $DownloadCore) {
    Write-Host "    Downloading latest MetaCubeX/mihomo Windows amd64 release..." -ForegroundColor Yellow
    $headers = @{ "User-Agent" = "mihomo-desktop-builder" }
    if ($env:GITHUB_TOKEN) {
        $headers["Authorization"] = "Bearer $($env:GITHUB_TOKEN)"
    }
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/MetaCubeX/mihomo/releases/latest" -Headers $headers
    $asset = $release.assets | Where-Object { $_.name -match '^mihomo-windows-amd64-v2-v[0-9].*\.zip$' } | Select-Object -First 1
    if (-not $asset) {
        $asset = $release.assets | Where-Object { $_.name -match '^mihomo-windows-amd64-compatible-v[0-9].*\.zip$' } | Select-Object -First 1
    }
    if (-not $asset) {
        $asset = $release.assets | Where-Object { $_.name -match '^mihomo-windows-amd64-v[0-9].*\.zip$' } | Select-Object -First 1
    }
    if (-not $asset) {
        throw "Could not find a suitable Windows amd64 zip asset in MetaCubeX/mihomo latest release ($($release.tag_name))"
    }

    $dlUrl = "${MirrorPrefix}$($asset.browser_download_url)"
    $tmpZip = Join-Path $env:TEMP "mihomo-core-$($release.tag_name).zip"
    $tmpExtract = Join-Path $env:TEMP "mihomo-core-extract-$([guid]::NewGuid().ToString('N'))"
    Write-Host "    Fetching $($asset.name) from $dlUrl ..." -ForegroundColor Yellow
    curl.exe -fsSL --retry 3 --connect-timeout 20 -o $tmpZip $dlUrl
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $tmpZip)) {
        throw "Failed to download mihomo core archive from $dlUrl"
    }
    Expand-Archive -Path $tmpZip -DestinationPath $tmpExtract -Force
    $extractedExe = Get-ChildItem -Path $tmpExtract -Recurse -Filter "*.exe" | Select-Object -First 1
    if (-not $extractedExe) {
        throw "No .exe found inside downloaded mihomo archive"
    }
    $targetCore = Join-Path $RepoRoot "installer\mihomo.exe"
    Copy-Item -Force $extractedExe.FullName $targetCore
    Remove-Item -Force $tmpZip -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $tmpExtract -ErrorAction SilentlyContinue
    $ResolvedCore = (Resolve-Path $targetCore).Path
}

if (-not $ResolvedCore) {
    throw "mihomo.exe not found. Pass -CorePath <path-to-mihomo.exe>, place it in installer\mihomo.exe, or pass -DownloadCore"
}
Write-Host "    Using core: $ResolvedCore" -ForegroundColor Green

Write-Host "==> 3. Ensuring bundled GEO / MMDB / ASN databases..." -ForegroundColor Cyan
$GeoCacheDir = Join-Path $RepoRoot "installer\geo"
New-Item -ItemType Directory -Force -Path $GeoCacheDir | Out-Null
$GeoAssets = @(
    @{ Name = "GeoIP.dat";    Url = "${MirrorPrefix}https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/geoip.dat" },
    @{ Name = "GeoSite.dat";  Url = "${MirrorPrefix}https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/geosite.dat" },
    @{ Name = "geoip.metadb"; Url = "${MirrorPrefix}https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/geoip.metadb" },
    @{ Name = "ASN.mmdb";     Url = "${MirrorPrefix}https://github.com/xishang0128/geoip/releases/download/latest/GeoLite2-ASN.mmdb" }
)
foreach ($asset in $GeoAssets) {
    $destPath = Join-Path $GeoCacheDir $asset.Name
    $needsDownload = $RefreshGeo -or (-not (Test-Path $destPath)) -or ((Get-Item $destPath).Length -eq 0)
    if ($needsDownload) {
        Write-Host "    Downloading $($asset.Name) from $($asset.Url) ..." -ForegroundColor Yellow
        curl.exe -fsSL --retry 3 --connect-timeout 20 -o $destPath $asset.Url
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path $destPath) -or (Get-Item $destPath).Length -eq 0) {
            throw "Failed to download GEO resource: $($asset.Name)"
        }
    } else {
        Write-Host "    Using cached $($asset.Name) ($((Get-Item $destPath).Length) bytes)" -ForegroundColor Green
    }
}

Write-Host "==> 4. Staging bundle into dist\stage..." -ForegroundColor Cyan
$StageDir = Join-Path $RepoRoot "dist\stage"
$StageGeoDir = Join-Path $StageDir "geo"
New-Item -ItemType Directory -Force -Path $StageDir | Out-Null
New-Item -ItemType Directory -Force -Path $StageGeoDir | Out-Null
Copy-Item -Force $DesktopExe (Join-Path $StageDir "mihomo-desktop.exe")
Copy-Item -Force $ResolvedCore (Join-Path $StageDir "mihomo.exe")
foreach ($asset in $GeoAssets) {
    Copy-Item -Force (Join-Path $GeoCacheDir $asset.Name) (Join-Path $StageGeoDir $asset.Name)
}

# Also generate standalone Install.cmd / Uninstall.cmd inside dist\stage so it can be installed even without Inno Setup
@'
@echo off
setlocal
cd /d "%~dp0"
echo Installing Mihomo Desktop + Windows Service (requires Administrator)...
powershell -NoProfile -Command "Start-Process -FilePath '%~dp0mihomo-desktop.exe' -ArgumentList '--install-service --service-name mihomo --core \"C:\Program Files\Mihomo\mihomo.exe\" --work-dir \"C:\ProgramData\Mihomo\" --config \"C:\ProgramData\Mihomo\config\clash.yaml\" --bundled-core \"%~dp0mihomo.exe\" --bundled-geo-dir \"%~dp0geo\"' -Verb RunAs -Wait"
echo Done! Launching Mihomo Desktop...
start "" "C:\Program Files\Mihomo\mihomo-desktop.exe"
'@ | Set-Content -Encoding ASCII (Join-Path $StageDir "Install.cmd")

@'
@echo off
setlocal
echo Uninstalling Mihomo Windows Service (requires Administrator)...
powershell -NoProfile -Command "Start-Process -FilePath 'C:\Program Files\Mihomo\mihomo-desktop.exe' -ArgumentList '--uninstall-service --service-name mihomo' -Verb RunAs -Wait"
echo Service uninstalled.
pause
'@ | Set-Content -Encoding ASCII (Join-Path $StageDir "Uninstall.cmd")

$PortableZip = Join-Path $RepoRoot "dist\MihomoDesktop-$Version-x64-portable.zip"
if (Test-Path $PortableZip) { Remove-Item -Force $PortableZip }
Compress-Archive -Path (Join-Path $StageDir "*") -DestinationPath $PortableZip -CompressionLevel Optimal
Write-Host "    Portable archive ready: $PortableZip" -ForegroundColor Green

Write-Host "==> 5. Looking for Inno Setup 6 (ISCC.exe)..." -ForegroundColor Cyan
$Iscc = $null
$IsccCmd = Get-Command iscc -ErrorAction SilentlyContinue
if ($IsccCmd) {
    $Iscc = $IsccCmd.Source
} else {
    foreach ($p in @(
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
    )) {
        if (Test-Path $p) { $Iscc = $p; break }
    }
}

if ($Iscc) {
    Write-Host "    Compiling Inno Setup installer (v$Version) with $Iscc ..." -ForegroundColor Green
    & $Iscc "/DMyAppVersion=$Version" (Join-Path $RepoRoot "installer\mihomo-desktop.iss")
    if ($LASTEXITCODE -ne 0) {
        throw "ISCC compilation failed"
    }
    Write-Host "==> Installer ready in dist\ !" -ForegroundColor Green
} else {
    Write-Host "    Inno Setup 6 (ISCC.exe) not found." -ForegroundColor Yellow
    Write-Host "    Standalone bundle is ready in: $StageDir (run Install.cmd to install)." -ForegroundColor Yellow
    Write-Host "    To compile a single MihomoDesktop-Setup.exe, install Inno Setup 6 via:" -ForegroundColor Yellow
    Write-Host "      winget install JRSoftware.InnoSetup" -ForegroundColor White
    Write-Host "    and re-run: .\scripts\build-installer.ps1 -SkipBuild" -ForegroundColor White
}
