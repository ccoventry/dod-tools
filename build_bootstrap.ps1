$files = @(
    ".cursorrules",
    "docs/ai_rules/ai_architecture_protocols.md",
    "docs/ai_rules/ai_execution_protocols.md",
    "docs/architecture.md",
    "docs/domain_quirks.md",
    "docs/milestones.md",
    "docs/active_context.md"
)
$output = "bootstrap_payload.md"
Clear-Content $output -ErrorAction SilentlyContinue

foreach ($file in $files) {
    if (Test-Path $file) {
        Add-Content $output "`n---`n# File: $file`n"
        Get-Content $file | Add-Content $output
    } else {
        Write-Warning "[dod] Skipping missing file: $file"
    }
}
Write-Host "[dod] bootstrap_payload.md compiled successfully."
