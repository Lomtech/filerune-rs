# Baut FileRune und legt build\FileRune.exe ab — das Gegenstück zu bundle.sh
# auf macOS, damit auf beiden Systemen derselbe Pfad herauskommt.
#
#   .\bundle.ps1                 nur bauen
#   .\bundle.ps1 -Install        zusätzlich ins Benutzerprofil installieren
#                                (%LOCALAPPDATA%\Programs\FileRune) plus
#                                Eintrag im Startmenü
#
# Bewusst ohne [CmdletBinding()]: -Debug und -Verbose sind dort belegte Namen
# und würden mit einem eigenen Schalter kollidieren.

param(
    [ValidateSet('release', 'debug')]
    [string]$Configuration = 'release',
    [switch]$Install
)

$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot

$AppName = 'FileRune'

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo nicht gefunden. Rust installieren: https://rustup.rs"
}

Write-Host "-> cargo build ($Configuration)"
if ($Configuration -eq 'release') { cargo build --release } else { cargo build }
if ($LASTEXITCODE -ne 0) { throw "cargo build fehlgeschlagen." }

# cargo benennt die Datei nach dem Paket (klein), das Bundle nach der App.
$Source = Join-Path 'target' (Join-Path $Configuration 'filerune.exe')
if (-not (Test-Path -LiteralPath $Source)) {
    throw "Nicht gefunden: $Source"
}

$BuildDir = 'build'
New-Item -ItemType Directory -Force -Path $BuildDir | Out-Null
$Staged = Join-Path $BuildDir "$AppName.exe"
Copy-Item -LiteralPath $Source -Destination $Staged -Force
Write-Host "OK  $Staged"

if (-not $Install) {
    Write-Host "    Starten mit:   .\build\$AppName.exe"
    Write-Host "    Installieren:  .\bundle.ps1 -Install"
    return
}

$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\$AppName"
$InstalledExe = Join-Path $InstallDir "$AppName.exe"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

try {
    Copy-Item -LiteralPath $Staged -Destination $InstalledExe -Force
}
catch {
    throw "Konnte nicht nach $InstallDir kopieren — läuft $AppName noch? Erst beenden, dann erneut."
}

# Startmenü-Verknüpfung. Das Icon steckt schon in der .exe (siehe build.rs),
# die Verknüpfung erbt es.
$StartMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
New-Item -ItemType Directory -Force -Path $StartMenu | Out-Null
$LinkPath = Join-Path $StartMenu "$AppName.lnk"

$Shell = New-Object -ComObject WScript.Shell
$Link = $Shell.CreateShortcut($LinkPath)
$Link.TargetPath = $InstalledExe
$Link.WorkingDirectory = $InstallDir
$Link.Description = 'Tastaturgetriebener Dateimanager'
$Link.Save()

Write-Host "OK  installiert nach $InstallDir"
Write-Host "OK  Startmenü-Eintrag: $LinkPath"
Write-Host "    Deinstallieren: Ordner und Verknüpfung löschen."
