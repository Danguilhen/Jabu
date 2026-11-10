# Stage the GStreamer and MSVC runtime used by the Windows desktop build.
#
# The NSIS installer hook (dev/windows/nsis-runtime.nsh) and the portable zip
# both read dist/windows-runtime, so run this before `dx bundle`.
#
# Usage:
#   .\dev\prepare-windows-runtime.ps1
#   .\dev\prepare-windows-runtime.ps1 -GStreamerRoot "C:\Program Files\gstreamer\1.0\msvc_x86_64"
#
# On CI the GStreamer tree is already in $env:GSTREAMER_ROOT.

[CmdletBinding()]
param(
    # Root of the GStreamer MSVC x86_64 tree (the directory containing bin/).
    [string]$GStreamerRoot,
    # Optional VC\Redist\MSVC root, or an x64 directory below it.
    [string]$MsvcRedistRoot
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Stage = Join-Path (Join-Path $RepoRoot "dist") "windows-runtime"

function Test-GStreamerTree([string]$Root) {
    $bin = Join-Path $Root "bin"
    return (Test-Path (Join-Path $bin "gstapp-1.0-0.dll"))
}

function Find-GStreamerRoot {
    if ($GStreamerRoot) {
        if (Test-GStreamerTree $GStreamerRoot) {
            return (Resolve-Path $GStreamerRoot).Path
        }
        throw "GStreamerRoot does not contain bin\gstapp-1.0-0.dll: $GStreamerRoot"
    }

    if ($env:GSTREAMER_ROOT) {
        if (Test-GStreamerTree $env:GSTREAMER_ROOT) {
            return (Resolve-Path $env:GSTREAMER_ROOT).Path
        }
        throw "GSTREAMER_ROOT does not contain bin\gstapp-1.0-0.dll: $env:GSTREAMER_ROOT"
    }

    $bases = @(
        "C:\gstreamer",
        (Join-Path $env:ProgramFiles "gstreamer")
    )
    $x86ProgramFiles = ${env:ProgramFiles(x86)}
    if ($x86ProgramFiles) {
        $bases += Join-Path $x86ProgramFiles "gstreamer"
    }

    $pc = Get-ChildItem $bases -Recurse -Filter glib-2.0.pc -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if (-not $pc) {
        throw "GStreamer not found. Install the 1.26.11 MSVC x86_64 runtime and development packages, or pass -GStreamerRoot."
    }

    # ...\lib\pkgconfig\glib-2.0.pc -> ...\lib\pkgconfig -> ...\lib -> root
    return $pc.Directory.Parent.Parent.FullName
}

function Find-MsvcRedistRoot {
    if ($MsvcRedistRoot) {
        if (Test-Path $MsvcRedistRoot) {
            return (Resolve-Path $MsvcRedistRoot).Path
        }
        throw "MsvcRedistRoot does not exist: $MsvcRedistRoot"
    }

    $x86ProgramFiles = ${env:ProgramFiles(x86)}
    if (-not $x86ProgramFiles) {
        throw "ProgramFiles(x86) is not set; cannot locate the Visual Studio redistributable."
    }

    $vswhere = Join-Path (Join-Path $x86ProgramFiles "Microsoft Visual Studio") "Installer\vswhere.exe"
    if (-not (Test-Path $vswhere)) {
        throw "vswhere.exe not found. Install the Visual Studio C++ build tools or pass -MsvcRedistRoot."
    }

    $vs = & $vswhere -latest -products * `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
        -property installationPath
    if (-not $vs) {
        throw "vswhere could not find a Visual Studio installation with the C++ tools."
    }

    $redistRoot = Join-Path (Join-Path (Join-Path $vs "VC") "Redist") "MSVC"
    if (-not (Test-Path $redistRoot)) {
        throw "Visual Studio redistributable directory not found under $vs."
    }

    return $redistRoot
}

$GStreamerRoot = Find-GStreamerRoot
$MsvcRedistRoot = Find-MsvcRedistRoot

Write-Host "GStreamer root: $GStreamerRoot"
Write-Host "MSVC redist:   $MsvcRedistRoot"

if (Test-Path $Stage) {
    Remove-Item -Recurse -Force $Stage
}

$BinDir = Join-Path $Stage "bin"
$PluginDir = Join-Path (Join-Path $Stage "lib") "gstreamer-1.0"
$ScannerDir = Join-Path (Join-Path $Stage "libexec") "gstreamer-1.0"
New-Item -ItemType Directory -Force -Path $BinDir, $PluginDir, $ScannerDir | Out-Null

# Core GStreamer DLLs and their non-GStreamer dependencies.
Copy-Item (Join-Path (Join-Path $GStreamerRoot "bin") "*.dll") $BinDir

# Plugin DLLs are loaded from <prefix>/lib/gstreamer-1.0.
$PluginSource = Join-Path (Join-Path $GStreamerRoot "lib") "gstreamer-1.0"
Copy-Item (Join-Path $PluginSource "*.dll") $PluginDir

# gst-plugin-scanner.exe and gst-ptp-helper.exe run as child processes.
$ScannerSource = Join-Path (Join-Path $GStreamerRoot "libexec") "gstreamer-1.0"
if (-not (Test-Path $ScannerSource)) {
    throw "GStreamer plugin scanner directory not found: $ScannerSource"
}
Copy-Item (Join-Path $ScannerSource "*") $ScannerDir -Recurse

function Select-X64Directory($Candidates) {
    if (-not $Candidates) {
        return $null
    }

    # VS 2022/2026 place the CRT under <version>\x64\Microsoft.VC*.CRT.
    $x64 = $Candidates |
        Where-Object { $_.FullName -match '[\\/]x64[\\/]' } |
        Sort-Object FullName -Descending |
        Select-Object -First 1
    if ($x64) {
        return $x64
    }

    # Some redist layouts put the x64 CRT directly under the version directory.
    return $Candidates |
        Where-Object { $_.FullName -notmatch '[\\/](x86|arm64|spectre)[\\/]' } |
        Sort-Object FullName -Descending |
        Select-Object -First 1
}

# jabu.exe and gio-2.0-0.dll import VCRUNTIME140*.dll. llama.cpp's OpenMP build
# also needs vcomp140.dll. App-local deployment keeps the installer independent
# of a machine-wide Visual C++ redistributable.
$crtCandidates = Get-ChildItem $MsvcRedistRoot -Recurse -Directory `
    -Filter "Microsoft.VC*.CRT" -ErrorAction SilentlyContinue
$crtDir = Select-X64Directory $crtCandidates
if (-not $crtDir) {
    throw "Microsoft VC CRT redistributable not found under $MsvcRedistRoot."
}
Write-Host "MSVC CRT:      $($crtDir.FullName)"
Copy-Item (Join-Path $crtDir.FullName "*.dll") $BinDir -Force

$openmpCandidates = Get-ChildItem $MsvcRedistRoot -Recurse -Directory `
    -Filter "Microsoft.VC*.OpenMP" -ErrorAction SilentlyContinue
$openmpDir = Select-X64Directory $openmpCandidates
if ($openmpDir) {
    Write-Host "MSVC OpenMP:   $($openmpDir.FullName)"
    $vcomp = Join-Path $openmpDir.FullName "vcomp140.dll"
    if (Test-Path $vcomp) {
        Copy-Item $vcomp $BinDir -Force
    }
}

$required = @(
    "bin/gstapp-1.0-0.dll",
    "bin/gstreamer-1.0-0.dll",
    "bin/gstsdp-1.0-0.dll",
    "bin/gstwebrtc-1.0-0.dll",
    "bin/vcruntime140.dll",
    "bin/vcruntime140_1.dll",
    "bin/vcomp140.dll",
    "lib/gstreamer-1.0/gstrswebrtc.dll",
    "libexec/gstreamer-1.0/gst-plugin-scanner.exe"
)
foreach ($relative in $required) {
    $path = Join-Path $Stage $relative
    if (-not (Test-Path $path)) {
        throw "The staged Windows runtime is missing $relative."
    }
}

$files = Get-ChildItem $Stage -Recurse -File
$sizeMb = ($files | Measure-Object Length -Sum).Sum / 1MB
Write-Host ("Staged {0} Windows runtime files ({1:N1} MB) in {2}" -f $files.Count, $sizeMb, $Stage)
