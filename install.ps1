# Lumina Windows Installer
# This script runs the installation in WSL Ubuntu

Write-Host "🔧 Lumina Installer (Windows)" -ForegroundColor Cyan
Write-Host ""

# Check if WSL is installed
try {
    $wslCheck = wsl --list --quiet 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "❌ WSL not found. Please install WSL first:" -ForegroundColor Red
        Write-Host "   wsl --install" -ForegroundColor Yellow
        exit 1
    }
} catch {
    Write-Host "❌ WSL not found. Please install WSL first:" -ForegroundColor Red
    Write-Host "   wsl --install" -ForegroundColor Yellow
    exit 1
}

# Check if Ubuntu is installed
$ubuntuInstalled = wsl --list --quiet | Select-String -Pattern "Ubuntu"
if (-not $ubuntuInstalled) {
    Write-Host "❌ Ubuntu not found in WSL. Please install:" -ForegroundColor Red
    Write-Host "   wsl --install -d Ubuntu" -ForegroundColor Yellow
    exit 1
}

# Get current directory as WSL path
$currentDir = Get-Location
$wslPath = $currentDir.Path -replace '\\', '/' -replace 'C:', '/mnt/c'

Write-Host "📂 Project location: $currentDir" -ForegroundColor Green
Write-Host "📂 WSL path: $wslPath" -ForegroundColor Green
Write-Host ""

# Run install script in WSL
Write-Host "🚀 Running installer in WSL Ubuntu..." -ForegroundColor Cyan
wsl -d Ubuntu bash "$wslPath/install.sh"

if ($LASTEXITCODE -eq 0) {
    Write-Host ""
    Write-Host "✅ Installation complete!" -ForegroundColor Green
    Write-Host ""
    Write-Host "🎯 Next steps:" -ForegroundColor Cyan
    Write-Host "   1. Get Voyage API key: https://www.voyageai.com/" -ForegroundColor White
    Write-Host "   2. In WSL, run: export VOYAGE_API_KEY='pa-your-key'" -ForegroundColor White
    Write-Host "   3. Index a repo: wsl lumina index --repo /mnt/c/path/to/repo" -ForegroundColor White
    Write-Host ""
    Write-Host "📘 For Claude Code integration, see README.md" -ForegroundColor Yellow
} else {
    Write-Host ""
    Write-Host "❌ Installation failed" -ForegroundColor Red
    exit 1
}
