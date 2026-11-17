# Start Thread Vision with the Basler pylon runtime on PATH. Run from the repository root:
#   .\run_app.ps1
# (builds the release app first if needed)
$env:PATH += ";C:\Program Files\Basler\pylon 7\Runtime\x64;$env:USERPROFILE\.cargo\bin"
cargo build --release -p app
if ($LASTEXITCODE -eq 0) { & .\target\release\app.exe }
