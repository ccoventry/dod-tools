$ExePath = $args[0]
$ExeArgs = $args[1..($args.Count - 1)]

$cert = Get-Item Cert:\CurrentUser\My\* | Where-Object { $_.Subject -match 'LocalRustDev' } | Select-Object -First 1
if ($cert -and (Test-Path $ExePath)) {
    Set-AuthenticodeSignature -Certificate $cert -FilePath $ExePath | Out-Null
}

if ($ExeArgs) {
    & $ExePath $ExeArgs
} else {
    & $ExePath
}
