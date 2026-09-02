# Build ClaudePet + its installer and publish a GitHub Release that the app's
# auto-updater reads. Requires the GitHub CLI (`gh auth login` done once).
#
#   .\publish.ps1                # version taken from Cargo.toml
#   .\publish.ps1 -Version 0.2.0 # explicit
#
# Auto-upload alternative: push a tag `vX.Y.Z` and .github/workflows/release.yml
# builds + uploads the same three assets on a GitHub runner.

param(
    [string]$Version,
    [string]$Repo = "emm312/claudepet",
    [string]$Branch = "windows"
)
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot   # src-win/

if (-not $Version) {
    $m = Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"' | Select-Object -First 1
    if (-not $m) { throw "could not read version from Cargo.toml; pass -Version" }
    $Version = $m.Matches[0].Groups[1].Value
}
$tag = "v$Version"
Write-Host "Publishing $tag to $Repo ($Branch)" -ForegroundColor Cyan

cargo build --release -p claudepet
cargo build --release -p claudepet-setup

$exe   = "target/release/claudepet.exe"
$setup = "target/release/claudepet-setup.exe"
if (-not (Test-Path $exe))   { throw "missing $exe" }
if (-not (Test-Path $setup)) { throw "missing $setup" }

# Checksum the app exe - the updater verifies this before swapping.
$sha = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLower()
Set-Content -Path "$exe.sha256" -Value $sha -Encoding ascii -NoNewline

# Create (or replace) the release and upload assets. Asset name MUST stay
# `claudepet.exe` / `claudepet.exe.sha256` - update::check() looks them up by name.
$exists = (& gh release view $tag --repo $Repo 2>$null)
if ($LASTEXITCODE -eq 0) {
    gh release upload $tag $exe $setup "$exe.sha256" --repo $Repo --clobber
} else {
    gh release create $tag $exe $setup "$exe.sha256" `
        --repo $Repo --target $Branch --title $tag `
        --notes "ClaudePet $tag"
}
Write-Host "Done. Running $($Version) clients will update on their next check." -ForegroundColor Green
