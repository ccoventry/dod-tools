$ExePath = $args[0]
$ExeArgs = $args[1..($args.Count - 1)]

# 1. Locate or create self-signed code signing certificate
$cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -match 'LocalRustDev' } | Select-Object -First 1

if (-not $cert) {
    Write-Host "Creating self-signed CodeSigning certificate 'CN=LocalRustDev'..."
    $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=LocalRustDev" -CertStoreLocation "Cert:\CurrentUser\My"
}

# 2. Ensure certificate is trusted in CurrentUser Root store (bypasses Smart App Control blocking)
if ($cert) {
    $rootCert = Get-ChildItem Cert:\CurrentUser\Root | Where-Object { $_.Thumbprint -eq $cert.Thumbprint }
    if (-not $rootCert) {
        Write-Host "Adding certificate to CurrentUser Root (Trusted Root Certification Authorities)..."
        $store = New-Object System.Security.Cryptography.X509Certificates.X509Store("Root", "CurrentUser")
        $store.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
        $store.Add($cert)
        $store.Close()
    }
}

# 3. Apply Authenticode signature
if ($cert -and (Test-Path $ExePath)) {
    $sigResult = Set-AuthenticodeSignature -Certificate $cert -FilePath $ExePath
    Write-Host "Signed $ExePath with status: $($sigResult.Status)"
}

# 4. Invoke target executable
if ($ExeArgs) {
    & $ExePath $ExeArgs
} else {
    & $ExePath
}
