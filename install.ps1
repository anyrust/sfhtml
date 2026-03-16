# sfhtml installer for Windows — downloads the latest release binary
# Usage: irm https://raw.githubusercontent.com/anyrust/sfhtml/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "anyrust/sfhtml"
$InstallDir = if ($env:SFHTML_INSTALL_DIR) { $env:SFHTML_INSTALL_DIR } else { "$env:USERPROFILE\.sfhtml\bin" }

# Detect architecture
$Arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
switch ($Arch) {
    "X64"   { $ArchName = "x86_64" }
    "Arm64" { $ArchName = "aarch64" }
    default { Write-Error "Unsupported architecture: $Arch"; exit 1 }
}

$Archive = "sfhtml-windows-${ArchName}.zip"

# Get latest release tag
Write-Host "Fetching latest release..."
$Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
$Tag = $Release.tag_name

if (-not $Tag) {
    Write-Error "Could not determine latest release. Install manually: cargo install sfhtml"
    exit 1
}

$Url = "https://github.com/$Repo/releases/download/$Tag/$Archive"
Write-Host "Downloading sfhtml $Tag for windows-$ArchName..."
Write-Host "  $Url"

# Download and extract
$TmpDir = Join-Path ([System.IO.Path]::GetTempPath()) "sfhtml-install-$(Get-Random)"
New-Item -ItemType Directory -Path $TmpDir -Force | Out-Null

try {
    $ZipPath = Join-Path $TmpDir $Archive
    Invoke-WebRequest -Uri $Url -OutFile $ZipPath -UseBasicParsing
    Expand-Archive -Path $ZipPath -DestinationPath $TmpDir -Force

    # Create install directory if needed
    if (-not (Test-Path $InstallDir)) {
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    }

    # Copy binary
    $ExeSrc = Join-Path $TmpDir "sfhtml.exe"
    if (-not (Test-Path $ExeSrc)) {
        # Try nested folder
        $ExeSrc = Get-ChildItem -Path $TmpDir -Filter "sfhtml.exe" -Recurse | Select-Object -First 1 -ExpandProperty FullName
    }
    Copy-Item -Path $ExeSrc -Destination (Join-Path $InstallDir "sfhtml.exe") -Force

    Write-Host ""
    Write-Host "sfhtml $Tag installed to $InstallDir\sfhtml.exe"

    # Add to PATH if not already present
    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($UserPath -notlike "*$InstallDir*") {
        [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
        Write-Host "Added $InstallDir to user PATH."
        Write-Host "Restart your terminal for PATH changes to take effect."
    }

    Write-Host ""
    Write-Host "Verify: sfhtml --version"
}
finally {
    Remove-Item -Path $TmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
