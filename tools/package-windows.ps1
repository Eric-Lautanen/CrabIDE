# PowerShell script to package crabide for Windows
# Produces: crabide-<version>-x86_64-windows.zip (portable)
# Optionally: crabide-<version>-x86_64-windows-installer.exe (NSIS)

param(
    [string]$TargetDir = "target/release",
    [string]$Version = "0.1.0",
    [switch]$BuildInstaller
)

$ErrorActionPreference = "Stop"

$BinaryName = "crabide.exe"
$SourceBinary = Join-Path $TargetDir $BinaryName
$DistDir = "dist/windows"

if (-not (Test-Path $SourceBinary)) {
    Write-Error "Binary not found at $SourceBinary. Run 'cargo build --release' first."
    exit 1
}

# Ensure dist directory exists
if (-not (Test-Path $DistDir)) {
    New-Item -ItemType Directory -Path $DistDir -Force | Out-Null
}

# Create portable zip
$ZipName = "crabide-$Version-x86_64-windows.zip"
$ZipPath = Join-Path $DistDir $ZipName

# Collect files to package
$Items = @(
    $SourceBinary,
    "assets/icon.ico",
    "README.md",
    "LICENSE-MIT",
    "LICENSE-APACHE"
)

# Create temporary staging directory
$StageDir = Join-Path $env:TEMP "crabide-pkg"
if (Test-Path $StageDir) { Remove-Item -Recurse -Force $StageDir }
New-Item -ItemType Directory -Path $StageDir -Force | Out-Null

foreach ($item in $Items) {
    if (Test-Path $item) {
        Copy-Item -Path $item -Destination $StageDir -Recurse
    }
}

# Create portable zip
Compress-Archive -Path "$StageDir\*" -DestinationPath $ZipPath -Force
Write-Host "Created portable zip: $ZipPath"

# Clean up staging
Remove-Item -Recurse -Force $StageDir

if ($BuildInstaller) {
    # Check for NSIS compiler
    $MakeNsis = Get-Command "makensis" -ErrorAction SilentlyContinue
    if (-not $MakeNsis) {
        Write-Warning "NSIS (makensis) not found. Skipping installer build."
        Write-Warning "Install NSIS from https://nsis.sourceforge.io/ or use --BuildInstaller:false"
        return
    }

    # Generate NSIS installer script
    $NsisScript = @"
!define PRODUCT_NAME "crabide"
!define PRODUCT_VERSION "$Version"
!define PRODUCT_PUBLISHER "crabide Contributors"
!define PRODUCT_WEB_SITE "https://crabide-editor.dev"
!define PRODUCT_UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
!define PRODUCT_DIR_REGKEY "Software\Microsoft\Windows\CurrentVersion\App Paths\crabide.exe"

Name "\${PRODUCT_NAME} \${PRODUCT_VERSION}"
OutFile "$DistDir\crabide-$Version-x86_64-windows-installer.exe"
InstallDir "\$PROGRAMFILES64\crabide"
RequestExecutionLevel admin

Section "Install"
  SetOutPath "\$INSTDIR"
  File "$SourceBinary"
  File "assets\icon.ico"
  File "README.md"
  File "LICENSE-MIT"
  File "LICENSE-APACHE"

  CreateDirectory "\$SMPROGRAMS\crabide"
  CreateShortCut "\$SMPROGRAMS\crabide\crabide.lnk" "\$INSTDIR\crabide.exe" "" "\$INSTDIR\icon.ico"
  CreateShortCut "\$DESKTOP\crabide.lnk" "\$INSTDIR\crabide.exe" "" "\$INSTDIR\icon.ico"

  WriteUninstaller "\$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "\${PRODUCT_DIR_REGKEY}" "" "\$INSTDIR\crabide.exe"
  WriteRegStr HKLM "\${PRODUCT_UNINSTALL_KEY}" "DisplayName" "\${PRODUCT_NAME}"
  WriteRegStr HKLM "\${PRODUCT_UNINSTALL_KEY}" "UninstallString" "\$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "\${PRODUCT_UNINSTALL_KEY}" "DisplayVersion" "\${PRODUCT_VERSION}"
  WriteRegStr HKLM "\${PRODUCT_UNINSTALL_KEY}" "Publisher" "\${PRODUCT_PUBLISHER}"
  WriteRegStr HKLM "\${PRODUCT_UNINSTALL_KEY}" "URLInfoAbout" "\${PRODUCT_WEB_SITE}"
SectionEnd

Section "Uninstall"
  Delete "\$INSTDIR\crabide.exe"
  Delete "\$INSTDIR\icon.ico"
  Delete "\$INSTDIR\README.md"
  Delete "\$INSTDIR\LICENSE-MIT"
  Delete "\$INSTDIR\LICENSE-APACHE"
  Delete "\$INSTDIR\uninstall.exe"
  RMDir "\$INSTDIR"

  Delete "\$SMPROGRAMS\crabide\crabide.lnk"
  RMDir "\$SMPROGRAMS\crabide"
  Delete "\$DESKTOP\crabide.lnk"

  DeleteRegKey HKLM "\${PRODUCT_DIR_REGKEY}"
  DeleteRegKey HKLM "\${PRODUCT_UNINSTALL_KEY}"
SectionEnd
"@

    $NsisPath = Join-Path $env:TEMP "crabide-installer.nsi"
    Set-Content -Path $NsisPath -Value $NsisScript -Encoding ASCII

    try {
        & makensis $NsisPath
        Write-Host "Created NSIS installer"
    } finally {
        Remove-Item $NsisPath -Force -ErrorAction SilentlyContinue
    }
}

Write-Host "Windows packaging complete."
