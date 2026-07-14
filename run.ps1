$ExeName = "dod-tools-gui"
$ExePath = "target\release\$ExeName.exe"

# 1. Proactively drop file locks
Stop-Process -Name $ExeName -Force -ErrorAction SilentlyContinue

# 2. Build the project
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# 3. Sign the binary to bypass WDAC
$cert = Get-Item Cert:\CurrentUser\My\* | Where-Object { $_.Subject -match 'LocalRustDev' } | Select-Object -First 1
if ($cert -and (Test-Path $ExePath)) {
    Set-AuthenticodeSignature -Certificate $cert -FilePath $ExePath | Out-Null
}

# 4. Execute and pass arguments
& $ExePath $args
