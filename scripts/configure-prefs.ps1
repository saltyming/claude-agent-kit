# configure-prefs.ps1 — Windows twin of configure-prefs.sh.
#
# Single prefs generator for every kit's install.ps1 (slate's POSIX
# configure-prefs.sh cannot run on Windows). Same behaviour as the shell
# version: INTERACTIVE-FIRST (Read-Host), ASKS before overwriting existing
# prefs, and injection-safe (literal String.Replace, never regex). Environment
# variables are optional seeds for the prompt defaults / non-interactive runs;
# nothing here requires one. Templates are resolved next to this script as
# <Prefix>--{aside,dispatch}-prefs.md.tmpl.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$RulesDir,
    [Parameter(Mandatory = $true)][string]$Prefix,
    [string]$Manifest = "",
    [switch]$NoPrompt
)
$ErrorActionPreference = "Stop"
$here = $PSScriptRoot

function Seed($name, $default = "") {
    $v = [Environment]::GetEnvironmentVariable($name)
    if (-not [string]::IsNullOrEmpty($v)) { return $v }
    return $default
}
function SeedFallback($main, $aliasName) {
    $v = [Environment]::GetEnvironmentVariable($main)
    if (-not [string]::IsNullOrEmpty($v)) { return $v }
    return (Seed $aliasName "")
}

$vals = [ordered]@{
    "ASIDE_PREFERRED"       = Seed "ASIDE_PREFERRED" "none"
    "ASIDE_CODEX_MODEL"     = Seed "ASIDE_CODEX_MODEL"
    "ASIDE_COPILOT_MODEL"   = Seed "ASIDE_COPILOT_MODEL"
    "ASIDE_CLAUDE_MODEL"    = Seed "ASIDE_CLAUDE_MODEL"
    "ASIDE_CODEX_EFFORT"    = Seed "ASIDE_CODEX_EFFORT"
    "ASIDE_COPILOT_EFFORT"  = Seed "ASIDE_COPILOT_EFFORT"
    "ASIDE_CLAUDE_EFFORT"   = Seed "ASIDE_CLAUDE_EFFORT"
    "ASIDE_CODEX_FALLBACK"  = SeedFallback "ASIDE_CODEX_FALLBACK" "ASIDE_CODEX_MODEL_FALLBACK"
    "ASIDE_COPILOT_FALLBACK" = SeedFallback "ASIDE_COPILOT_FALLBACK" "ASIDE_COPILOT_MODEL_FALLBACK"
    "ASIDE_CLAUDE_FALLBACK" = SeedFallback "ASIDE_CLAUDE_FALLBACK" "ASIDE_CLAUDE_MODEL_FALLBACK"
    "ASIDE_POLICY"          = Seed "ASIDE_POLICY" "conservative"
    "DISPATCH_POLICY"       = Seed "DISPATCH_POLICY" "conservative"
    "DISPATCH_APPROVAL"     = Seed "DISPATCH_APPROVAL" "ask"
    "DISPATCH_GRANULARITY"  = Seed "DISPATCH_GRANULARITY" "ask"
    "DISPATCH_BACKEND"      = Seed "DISPATCH_BACKEND"
    "DISPATCH_MODEL"        = Seed "DISPATCH_MODEL"
    "DISPATCH_EFFORT"       = Seed "DISPATCH_EFFORT"
    "DISPATCH_FALLBACK"     = Seed "DISPATCH_FALLBACK"
}

function Test-Interactive {
    return ((-not $NoPrompt) -and [Environment]::UserInteractive)
}

function Read-Var($key, $label) {
    if (-not (Test-Interactive)) { return }
    $cur = $vals[$key]
    $ans = Read-Host "$label [$cur]"
    if (-not [string]::IsNullOrEmpty($ans)) { $vals[$key] = $ans }
}

# $true to (re)generate; $false to keep. New file → generate; existing file →
# ASK (default keep). PREFS_RECONFIGURE overrides the prompt for CI.
function Test-WantGenerate($dest) {
    if (-not (Test-Path $dest)) { return $true }
    $env = [Environment]::GetEnvironmentVariable("PREFS_RECONFIGURE")
    if ($env -match '^(y|Y|yes|YES|Yes)$') { return $true }
    if ($env -match '^(n|N|no|NO|No)$') { return $false }
    if (-not (Test-Interactive)) { return $false }
    Write-Host ""
    Write-Host "Existing preferences found at:"
    Write-Host "  $dest"
    $ans = Read-Host "Reconfigure (overwrite)? [y/N]"
    return ($ans -match '^(y|Y|yes|YES|Yes)$')
}

function Add-Manifest($path) {
    if ([string]::IsNullOrEmpty($Manifest)) { return }
    $existing = if (Test-Path $Manifest) { Get-Content $Manifest } else { @() }
    if ($existing -notcontains $path) { Add-Content $Manifest $path }
}

function Write-Prefs($tmpl, $dest, $keys) {
    if (-not (Test-Path $tmpl)) { throw "template not found: $tmpl" }
    $content = Get-Content $tmpl -Raw
    foreach ($k in $keys) {
        # literal replace — safe for values containing regex/replacement metachars
        $content = $content.Replace("@@$k@@", [string]$vals[$k])
    }
    $dir = Split-Path -Parent $dest
    if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
    Set-Content -Path $dest -Value $content -NoNewline
    Write-Host "  prefs: $dest"
    Add-Manifest $dest
}

$asideKeys = @(
    "ASIDE_PREFERRED", "ASIDE_CODEX_MODEL", "ASIDE_COPILOT_MODEL", "ASIDE_CLAUDE_MODEL",
    "ASIDE_CODEX_EFFORT", "ASIDE_COPILOT_EFFORT", "ASIDE_CLAUDE_EFFORT",
    "ASIDE_CODEX_FALLBACK", "ASIDE_COPILOT_FALLBACK", "ASIDE_CLAUDE_FALLBACK", "ASIDE_POLICY"
)
$dispatchKeys = @(
    "DISPATCH_POLICY", "DISPATCH_APPROVAL", "DISPATCH_GRANULARITY",
    "DISPATCH_BACKEND", "DISPATCH_MODEL", "DISPATCH_EFFORT", "DISPATCH_FALLBACK"
)

$asideTmpl = Join-Path $here "$Prefix--aside-prefs.md.tmpl"
$dispatchTmpl = Join-Path $here "$Prefix--dispatch-prefs.md.tmpl"
$asideDest = Join-Path $RulesDir "$Prefix--aside-prefs.md"
$dispatchDest = Join-Path $RulesDir "$Prefix--dispatch-prefs.md"

# ── aside ──
if (Test-WantGenerate $asideDest) {
    if (Test-Interactive) {
        Write-Host "Configuring aside preferences (Enter keeps the shown default)."
        Read-Var "ASIDE_PREFERRED"      "aside preferred backend (none/codex/copilot/claude)"
        Read-Var "ASIDE_CODEX_MODEL"    "aside codex default model (blank = CLI default)"
        Read-Var "ASIDE_CODEX_EFFORT"   "aside codex reasoning effort (low/medium/high/xhigh, blank)"
        Read-Var "ASIDE_COPILOT_MODEL"  "aside copilot default model (blank = CLI default)"
        Read-Var "ASIDE_COPILOT_EFFORT" "aside copilot reasoning effort (low/medium/high/xhigh, blank)"
        Read-Var "ASIDE_CLAUDE_MODEL"   "aside claude default model (blank = CLI default)"
        Read-Var "ASIDE_CLAUDE_EFFORT"  "aside claude reasoning effort (low/medium/high/xhigh/max, blank)"
        Read-Var "ASIDE_CODEX_FALLBACK"   "aside codex fallback chain, comma-separated (blank = none)"
        Read-Var "ASIDE_COPILOT_FALLBACK" "aside copilot fallback chain, comma-separated (blank = none)"
        Read-Var "ASIDE_CLAUDE_FALLBACK"  "aside claude fallback chain, comma-separated (blank = none)"
        Read-Var "ASIDE_POLICY"         "aside auto-call policy (conservative/preference-only/proactive)"
    }
    Write-Prefs $asideTmpl $asideDest $asideKeys
} else {
    Write-Host "  prefs exist, keeping: $asideDest"
    Add-Manifest $asideDest
}

# ── dispatch ──
if (Test-WantGenerate $dispatchDest) {
    if (Test-Interactive) {
        Write-Host "Configuring dispatch preferences (Enter keeps the shown default)."
        Read-Var "DISPATCH_POLICY"      "dispatch execution policy (conservative/preference-only/proactive)"
        Read-Var "DISPATCH_APPROVAL"    "dispatch approval mode (ask/auto)"
        Read-Var "DISPATCH_GRANULARITY" "dispatch default granularity (per-step/batch/ask)"
        Read-Var "DISPATCH_BACKEND"     "dispatch default backend (codex/opencode/claude, blank)"
        Read-Var "DISPATCH_MODEL"       "dispatch default model (blank = backend default)"
        Read-Var "DISPATCH_EFFORT"      "dispatch default reasoning effort (low/medium/high/xhigh, blank)"
        Read-Var "DISPATCH_FALLBACK"    "dispatch fallback chain, comma-separated (blank = none)"
    }
    Write-Prefs $dispatchTmpl $dispatchDest $dispatchKeys
} else {
    Write-Host "  prefs exist, keeping: $dispatchDest"
    Add-Manifest $dispatchDest
}
