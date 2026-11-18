# Installer build (M13.3): compiles scripts/installer/camcraft.iss with
# Inno Setup against an already-staged dist tree (the zip's) and writes
# dist/camcraft-<ver>-win-x64-setup.exe beside the zip. With a certificate
# configured (CAMCRAFT_SIGN_*, scripts/sign.ps1) ISCC signs the setup exe
# and its embedded uninstaller; without one both stay unsigned, as before.
#
# Usage: powershell -File scripts/installer.ps1 -Version 0.8.0 `
#            [-Stage dist/camcraft-0.8.0-win-x64] [-OutDir dist] [-Required]
# Without -Required a missing ISCC.exe is a named skip (exit 0) - the local
# cut must not depend on Inno Setup being installed. With -Required (CI,
# where the runner image ships Inno Setup) a missing ISCC.exe is an error.
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [string]$Stage = "",
    [string]$OutDir = "",
    [switch]$Required
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

function Fail([string]$msg) {
    Write-Host "INSTALLER FAILED: $msg"
    exit 1
}

if ($Version -notmatch '^\d+\.\d+\.\d+$') { Fail "-Version must be X.Y.Z, got '$Version'" }
if (-not $Stage) { $Stage = Join-Path $root "dist/camcraft-$Version-win-x64" }
if (-not $OutDir) { $OutDir = Join-Path $root "dist" }

# ---- 1. Find ISCC.exe: PATH, then the default Inno Setup 6 install dirs --
$iscc = $null
$cmd = Get-Command ISCC.exe -ErrorAction SilentlyContinue
if ($cmd) { $iscc = $cmd.Source }
if (-not $iscc) {
    foreach ($base in @(${env:ProgramFiles(x86)}, $env:ProgramFiles, (Join-Path $env:LOCALAPPDATA "Programs"))) {
        if (-not $base) { continue }
        $candidate = Join-Path $base "Inno Setup 6\ISCC.exe"
        if (Test-Path $candidate) { $iscc = $candidate; break }
    }
}
if (-not $iscc) {
    if ($Required) { Fail "ISCC.exe not found (Inno Setup 6 is required here)" }
    Write-Host "installer skipped: ISCC.exe (Inno Setup 6) not found; the zip is the release"
    exit 0
}
Write-Host "ISCC: $iscc"

# ---- 2. The stage must be the zip's tree ---------------------------------
if (-not (Test-Path $Stage)) { Fail "stage dir not found: $Stage (assemble the zip first)" }
foreach ($must in @("camcraft-ui.exe", "post.exe", "assets\themes", "examples\first-part.camproj", "examples\models\first-part.obj", "TESTING.md", "RELEASE_NOTES.md")) {
    if (-not (Test-Path (Join-Path $Stage $must))) { Fail "stage is missing $must" }
}
New-Item -ItemType Directory -Force $OutDir | Out-Null
$stageAbs = (Resolve-Path $Stage).Path
$outAbs = (Resolve-Path $OutDir).Path
$iss = Join-Path $PSScriptRoot "installer\camcraft.iss"

# ---- 3. Signing: sign.ps1 decides, ISCC calls it ------------------------
# sign.ps1 -Status is the one reader of CAMCRAFT_SIGN_*. Configured, ISCC
# gets a sign tool named "camcraft" that runs sign.ps1 on each file it
# signs ($f = the quoted file, $q = a quote; Inno Setup SignTool syntax),
# and the .iss turns on SignTool + SignedUninstaller, so the setup exe AND
# the uninstaller embedded in it are signed by the same signtool command
# and timestamp server as the release exes. Not configured, no sign tool
# is passed and the build is byte-for-byte today's unsigned one.
$signScript = Join-Path $PSScriptRoot "sign.ps1"
& powershell -NoProfile -ExecutionPolicy Bypass -File $signScript -Status
$signStatus = $LASTEXITCODE
$signArgs = @()
if ($signStatus -eq 0) {
    # Single-quoted: $q and $f are ISCC's placeholders, not PowerShell's.
    $tool = 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File $q' + $signScript + '$q -Path $f'
    $signArgs = @("/DSignToolName=camcraft", "/Scamcraft=$tool")
    Write-Host "installer signing: on (setup exe and embedded uninstaller, via sign.ps1)"
} elseif ($signStatus -eq 10) {
    Write-Host "installer signing: off (no certificate configured); setup and uninstaller ship unsigned"
} else {
    Fail "sign.ps1 -Status exited with $signStatus (signing half-configured)"
}

# ---- 4. Compile ---------------------------------------------------------
$isccArgs = @("/Qp", "/DAppVersion=$Version", "/DStageDir=$stageAbs", "/DOutDir=$outAbs") + $signArgs + @($iss)
Write-Host "> ISCC $($isccArgs -join ' ')"
& $iscc @isccArgs
if ($LASTEXITCODE -ne 0) { Fail "ISCC exited with $LASTEXITCODE" }
$setup = Join-Path $outAbs "camcraft-$Version-win-x64-setup.exe"
if (-not (Test-Path $setup)) { Fail "ISCC succeeded but $setup is missing" }
if ($signArgs.Count -gt 0) {
    # sign.ps1 already ran signtool verify on each file ISCC handed it; this
    # catches an ISCC that silently skipped the sign tool.
    $sig = Get-AuthenticodeSignature $setup
    if (-not $sig.SignerCertificate) { Fail "signing is configured but $setup carries no signature" }
    Write-Host "setup signed by: $($sig.SignerCertificate.Subject)"
}
Write-Host "Installer: $setup"
exit 0
