# install.ps1 — Windows installer for claude-agent-kit
# Usage:
#   iwr https://raw.githubusercontent.com/saltyming/claude-agent-kit/main/install.ps1 | iex
#   iwr https://raw.githubusercontent.com/saltyming/claude-agent-kit/main/install.ps1 -OutFile install.ps1; .\install.ps1 -Uninstall

param([switch]$Uninstall)

$ErrorActionPreference = "Stop"

$Repo      = "saltyming/claude-agent-kit"
$Branch    = "main"
$RawBase   = "https://raw.githubusercontent.com/$Repo/$Branch"
$ClaudeDir = Join-Path $env:USERPROFILE ".claude"
$RulesDir  = Join-Path $ClaudeDir "rules"
$BinDir    = Join-Path $env:USERPROFILE ".local\bin"
$Manifest  = Join-Path $ClaudeDir ".claude-agent-kit-manifest"
$Signature = "claude-agent-kit"

$RuleFiles = @(
    "claude-agent-kit--task-execution.md"
    "claude-agent-kit--git-workflow.md"
    "claude-agent-kit--framework-conventions.md"
    "claude-agent-kit--parallel-work.md"
)

function Do-Uninstall {
    if (-not (Test-Path $Manifest)) {
        Write-Host "No manifest found. Nothing to uninstall."
        return
    }
    foreach ($f in Get-Content $Manifest) {
        if (Test-Path $f) {
            if ($f -like "*.md") {
                $first = Get-Content $f -TotalCount 1
                if ($first -match $Signature) {
                    Remove-Item $f -Force
                    Write-Host "  removed $f"
                } else {
                    Write-Host "  skipped $f (signature mismatch)"
                }
            } else {
                Remove-Item $f -Force
                Write-Host "  removed $f"
            }
        }
    }
    Remove-Item $Manifest -Force
    if (Get-Command claude -ErrorAction SilentlyContinue) {
        try {
            claude mcp remove workslate -s user 2>$null
            Write-Host "  MCP server unregistered."
        } catch {}
    }
    Write-Host "Uninstalled."
}

if ($Uninstall) {
    Do-Uninstall
    return
}

Write-Host "Installing claude-agent-kit..."

# Detect architecture
$arch = if ([Environment]::Is64BitOperatingSystem) {
    if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "aarch64" } else { "x86_64" }
} else {
    Write-Host "Error: 32-bit systems not supported"; exit 1
}
$platform = "$arch-pc-windows-msvc"

New-Item -ItemType Directory -Force -Path $RulesDir | Out-Null
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
Set-Content $Manifest -Value ""

# Binary from latest GitHub Release
Write-Host "Downloading workslate binary ($platform)..."
$releaseUrl = "https://github.com/$Repo/releases/latest/download/workslate-$platform.zip"
$tmp = New-TemporaryFile | Rename-Item -NewName { $_.Name + ".zip" } -PassThru
Invoke-WebRequest -Uri $releaseUrl -OutFile $tmp.FullName
$extractDir = Join-Path $env:TEMP "claude-agent-kit-extract"
Expand-Archive -Path $tmp.FullName -DestinationPath $extractDir -Force
$binDest = Join-Path $BinDir "workslate.exe"
Copy-Item (Join-Path $extractDir "workslate.exe") -Destination $binDest -Force
Add-Content $Manifest $binDest
Remove-Item $tmp.FullName -Force
Remove-Item $extractDir -Recurse -Force

# CLAUDE.md
Write-Host "Downloading CLAUDE.md..."
$claudeDest = Join-Path $ClaudeDir "CLAUDE.md"
Invoke-WebRequest -Uri "$RawBase/CLAUDE.md" -OutFile $claudeDest
Add-Content $Manifest $claudeDest

# Rule files
Write-Host "Downloading rules..."
foreach ($f in $RuleFiles) {
    $dest = Join-Path $RulesDir $f
    Invoke-WebRequest -Uri "$RawBase/claude-rules/$f" -OutFile $dest
    Add-Content $Manifest $dest
}

Write-Host ""
Write-Host "Installed:"
Write-Host "  Binary:  $binDest"
Write-Host "  Config:  $claudeDest"
Write-Host "  Rules:   $RulesDir\claude-agent-kit--*.md"
Write-Host ""

# PATH check
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$BinDir*") {
    Write-Host "WARNING: $BinDir is not in your PATH."
    Write-Host ""
    Write-Host "  Add it by running:"
    Write-Host "    [Environment]::SetEnvironmentVariable('Path', `"$BinDir;`$env:Path`", 'User')"
    Write-Host ""
    Write-Host "  Then restart your terminal."
    Write-Host ""
}

# Register MCP server
if (Get-Command claude -ErrorAction SilentlyContinue) {
    Write-Host "Registering workslate MCP server..."
    try {
        claude mcp add workslate -s user --transport stdio -- workslate 2>$null
        Write-Host "  MCP server registered."
    } catch {
        Write-Host "  MCP registration failed. Add manually: claude mcp add workslate -s user --transport stdio -- workslate"
    }
} else {
    Write-Host "Claude Code CLI not found. Register MCP server manually:"
    Write-Host "  claude mcp add workslate -s user --transport stdio -- workslate"
}

Write-Host ""
Write-Host "To uninstall, run:"
Write-Host "  iwr $RawBase/install.ps1 -OutFile install.ps1; .\install.ps1 -Uninstall"
