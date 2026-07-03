# build_bootstrap.ps1
param (
    [Parameter(Mandatory = $true)]
    [string]$Task,
    
    [Parameter(Mandatory = $false)]
    [string[]]$Files
)

$PayloadFile = "bootstrap_payload.md"

# 1. Initialize the markdown file with the specific task goal
$HeaderContent = @"
# ACTIVE CODING TASK CONTEXT
> Handled natively by global .cursorrules boundaries. Do not execute workspace sweeps.

## 🎯 Immediate Sprint Goal
$Task

"@

Set-Content -Path $PayloadFile -Value $HeaderContent -Encoding utf8

# 2. Append ONLY the specific source files provided in the terminal command
if ($Files) {
    foreach ($File in $Files) {
        if (Test-Path $File) {
            $RelativePath = Resolve-Path $File -Relative
            Add-Content -Path $PayloadFile -Value "## 📄 File Context: $RelativePath"
            Add-Content -Path $PayloadFile -Value "```rust"
            Add-Content -Path PayloadFile -Value (Get-Content -Path File -Raw)
            Add-Content -Path \$PayloadFile -Value "````n"
        }
        else {
            Write-Warning "File path not found: $File"
        }
    }
}

Write-Host "Success: $PayloadFile generated efficiently with targeted files." -ForegroundColor Green
