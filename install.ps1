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
$SkillsDir = Join-Path $ClaudeDir "skills"
$BinDir    = Join-Path $env:USERPROFILE ".local\bin"
$Manifest  = Join-Path $ClaudeDir ".claude-agent-kit-manifest"
$Signature       = "claude-agent-kit"
$CustomSignature = "claude-agent-kit-custom"

$RuleFiles = @(
    "claude-agent-kit--task-execution.md"
    "claude-agent-kit--git-workflow.md"
    "claude-agent-kit--framework-conventions.md"
    "claude-agent-kit--parallel-work.md"
    "claude-agent-kit--aside.md"
    "claude-agent-kit--dispatch.md"
    "claude-agent-kit--palette.md"
)

$Binaries = @("workslate", "aside", "dispatch")

$SkillNames = @("palette-init", "palette-spec", "palette-ux", "palette-ui", "palette-rules")

function Do-Uninstall {
    if (-not (Test-Path $Manifest)) {
        Write-Host "No manifest found. Nothing to uninstall."
        return
    }
    # Remove PreToolUse doorbell hooks while the binary still exists
    $workslateExe = Join-Path $BinDir "workslate.exe"
    if (Test-Path $workslateExe) {
        try { & $workslateExe --uninstall-hooks 2>$null } catch {}
    }
    $customList = @()
    foreach ($f in Get-Content $Manifest) {
        if (Test-Path $f -PathType Leaf) {
            if ($f -like "*.md") {
                $first = Get-Content $f -TotalCount 1
                if ($first -match [regex]::Escape("<!-- $CustomSignature")) {
                    $customList += $f
                } elseif ($first -match [regex]::Escape("<!-- $Signature -->")) {
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

    if ($customList.Count -gt 0) {
        Write-Host ""
        Write-Host "The following user-owned files were installed alongside the kit:"
        $customList | ForEach-Object { Write-Host "  $_" }
        $keep = $true
        if ($env:ASIDE_UNINSTALL_KEEP_PREFS) {
            if ($env:ASIDE_UNINSTALL_KEEP_PREFS -match '^(no|n|NO|N|No)$') { $keep = $false }
        } elseif ([Environment]::UserInteractive) {
            $answer = Read-Host "Remove these too? [y/N]"
            if ($answer -match '^(y|Y|yes|YES|Yes)$') { $keep = $false }
        }
        if (-not $keep) {
            foreach ($f in $customList) {
                Remove-Item $f -Force
                Write-Host "  removed $f"
            }
        } else {
            Write-Host ""
            Write-Host "Preserved (not managed by claude-agent-kit from this point on):"
            $customList | ForEach-Object { Write-Host "  $_" }
        }
    }

    # Remove palette skill directories recorded in the manifest (core-signed only)
    foreach ($d in Get-Content $Manifest) {
        if ($d -like "*\skills\palette-*" -and (Test-Path $d -PathType Container)) {
            $skillMd = Join-Path $d "SKILL.md"
            if ((Test-Path $skillMd) -and (Select-String -Path $skillMd -Pattern ([regex]::Escape("<!-- $Signature -->")) -Quiet)) {
                Remove-Item $d -Recurse -Force
                Write-Host "  removed $d"
            } else {
                Write-Host "  skipped $d (signature mismatch)"
            }
        }
    }
    Remove-Item $Manifest -Force
    if (Get-Command claude -ErrorAction SilentlyContinue) {
        foreach ($srv in $Binaries) {
            try {
                claude mcp remove $srv -s user 2>$null
                Write-Host "  $srv unregistered."
            } catch {}
        }
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

# Binaries from latest GitHub Release
foreach ($bin in $Binaries) {
    Write-Host "Downloading $bin binary ($platform)..."
    $releaseUrl = "https://github.com/$Repo/releases/latest/download/$bin-$platform.zip"
    $tmp = New-TemporaryFile | Rename-Item -NewName { $_.Name + ".zip" } -PassThru
    Invoke-WebRequest -Uri $releaseUrl -OutFile $tmp.FullName
    $extractDir = Join-Path $env:TEMP "claude-agent-kit-extract-$bin"
    if (Test-Path $extractDir) { Remove-Item $extractDir -Recurse -Force }
    Expand-Archive -Path $tmp.FullName -DestinationPath $extractDir -Force
    $binDest = Join-Path $BinDir "$bin.exe"
    Copy-Item (Join-Path $extractDir "$bin.exe") -Destination $binDest -Force
    Add-Content $Manifest $binDest
    Remove-Item $tmp.FullName -Force
    Remove-Item $extractDir -Recurse -Force
}

# Register PreToolUse doorbell hooks in settings.json
$workslateExe = Join-Path $BinDir "workslate.exe"
try {
    & $workslateExe --install-hooks
    if ($LASTEXITCODE -ne 0) { throw }
} catch {
    Write-Host "  Hook registration failed. Run manually: $workslateExe --install-hooks"
}

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


# Palette skills (each is a directory holding one SKILL.md)
Write-Host "Downloading palette skills..."
foreach ($s in $SkillNames) {
    $skillDir = Join-Path $SkillsDir $s
    New-Item -ItemType Directory -Force -Path $skillDir | Out-Null
    Invoke-WebRequest -Uri "$RawBase/claude-skills/$s/SKILL.md" -OutFile (Join-Path $skillDir "SKILL.md")
    Add-Content $Manifest $skillDir
}

Write-Host ""
Write-Host "Installed:"
Write-Host "  Binaries: $BinDir\workslate.exe, $BinDir\aside.exe, $BinDir\dispatch.exe"
Write-Host "  Config:   $claudeDest"
Write-Host "  Rules:    $RulesDir\claude-agent-kit--*.md"
Write-Host "  Skills:   $SkillsDir\palette-*"
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

# Register MCP servers
if (Get-Command claude -ErrorAction SilentlyContinue) {
    foreach ($srv in $Binaries) {
        Write-Host "Registering $srv MCP server..."
        try {
            claude mcp add $srv -s user --transport stdio -- $srv 2>$null
            Write-Host "  $srv registered."
        } catch {
            Write-Host "  $srv registration failed. Add manually: claude mcp add $srv -s user --transport stdio -- $srv"
        }
    }
} else {
    Write-Host "Claude Code CLI not found. Register MCP servers manually:"
    foreach ($srv in $Binaries) {
        Write-Host "  claude mcp add $srv -s user --transport stdio -- $srv"
    }
}

# ── Aside preferences (interactive) ──
$prefsDest = Join-Path $RulesDir "claude-agent-kit--aside-prefs.md"
$keepPrefs = $false

if (Test-Path $prefsDest) {
    $reconfigure = $null
    $envReconf = [Environment]::GetEnvironmentVariable("ASIDE_RECONFIGURE")
    if ($envReconf) {
        if ($envReconf -match '^(yes|y|YES|Yes|Y)$') { $reconfigure = $true }
        elseif ($envReconf -match '^(no|n|NO|No|N)$') { $reconfigure = $false }
    }
    if ($null -eq $reconfigure) {
        if ([Environment]::UserInteractive) {
            Write-Host ""
            Write-Host "Existing aside preferences found at:"
            Write-Host "  $prefsDest"
            $answer = Read-Host "Reconfigure (overwrite)? [y/N]"
            if ($answer -match '^(y|Y|yes|YES|Yes)$') { $reconfigure = $true } else { $reconfigure = $false }
        } else {
            $reconfigure = $false
        }
    }
    if (-not $reconfigure) {
        $keepPrefs = $true
        Write-Host "Keeping existing preferences (edit anytime at $prefsDest)."
        $existingManifest = if (Test-Path $Manifest) { Get-Content $Manifest } else { @() }
        if ($existingManifest -notcontains $prefsDest) {
            Add-Content $Manifest $prefsDest
        }
    }
}

function Prompt-WithDefault($label, $envName, $default, $validRegex) {
    $envVal = [Environment]::GetEnvironmentVariable($envName)
    if ($null -ne $envVal) { return $envVal }
    if (-not [Environment]::UserInteractive) { return $default }
    while ($true) {
        $answer = Read-Host $label
        if ([string]::IsNullOrEmpty($answer)) { $answer = $default }
        if ($validRegex -and ($answer -notmatch $validRegex)) {
            Write-Host "  invalid; accepted pattern: $validRegex"
            continue
        }
        return $answer
    }
}

if (-not $keepPrefs) {
    Write-Host ""
    Write-Host "Configuring claude-agent-kit aside preferences."
    Write-Host "(set ASIDE_* environment variables to run non-interactively)"
    Write-Host ""

    $preferred   = Prompt-WithDefault "Preferred third-party advisor [none/codex/copilot] (default none)" "ASIDE_PREFERRED"     "none"         '^(none|codex|copilot)$'
    $codexModel  = Prompt-WithDefault "Default model for codex (blank for CLI default)"                          "ASIDE_CODEX_MODEL"   ""             $null
    $codexEff    = Prompt-WithDefault "Codex reasoning effort [low/medium/high/xhigh, blank]"                    "ASIDE_CODEX_EFFORT"  ""             '^(low|medium|high|xhigh)?$'
    $copilotModel= Prompt-WithDefault "Default model for copilot (blank for CLI default)"                        "ASIDE_COPILOT_MODEL" ""             $null
    $copilotEff  = Prompt-WithDefault "Copilot reasoning effort [low/medium/high/xhigh, blank]"                  "ASIDE_COPILOT_EFFORT" ""            '^(low|medium|high|xhigh)?$'
    $policy      = Prompt-WithDefault "Auto-call policy [conservative/preference-only/proactive] (default conservative)" "ASIDE_POLICY" "conservative" '^(conservative|preference-only|proactive)$'
}

# Shared custom-rules dir prompt (used by both the aside and dispatch flows; the
# ingestion below runs once). Honors CUSTOM_RULES_DIR, falling back to ASIDE_CUSTOM_RULES_DIR.
if (-not [Environment]::GetEnvironmentVariable("CUSTOM_RULES_DIR") -and [Environment]::GetEnvironmentVariable("ASIDE_CUSTOM_RULES_DIR")) {
    [Environment]::SetEnvironmentVariable("CUSTOM_RULES_DIR", [Environment]::GetEnvironmentVariable("ASIDE_CUSTOM_RULES_DIR"))
}
$customRules = Prompt-WithDefault "Path to a directory of your own custom rule files (blank to skip)"        "CUSTOM_RULES_DIR" ""           $null

if (-not $keepPrefs) {
    # Render the template
    $tmplUrl = "$RawBase/scripts/claude-agent-kit--aside-prefs.md.tmpl"
    $tmplTmp = New-TemporaryFile
    Invoke-WebRequest -Uri $tmplUrl -OutFile $tmplTmp.FullName
    $tmplContent = Get-Content $tmplTmp.FullName -Raw
    $tmplContent = $tmplContent.Replace("{{PREFERRED_BACKEND}}", $preferred)
    $tmplContent = $tmplContent.Replace("{{CODEX_MODEL}}",       $codexModel)
    $tmplContent = $tmplContent.Replace("{{COPILOT_MODEL}}",     $copilotModel)
    $tmplContent = $tmplContent.Replace("{{CODEX_EFFORT}}",      $codexEff)
    $tmplContent = $tmplContent.Replace("{{COPILOT_EFFORT}}",    $copilotEff)
    $tmplContent = $tmplContent.Replace("{{POLICY}}",            $policy)

    Set-Content $prefsDest -Value $tmplContent -NoNewline
    Remove-Item $tmplTmp.FullName -Force

    # Avoid duplicate manifest entries if re-running
    $existingManifest = if (Test-Path $Manifest) { Get-Content $Manifest } else { @() }
    if ($existingManifest -notcontains $prefsDest) {
        Add-Content $Manifest $prefsDest
    }
    Write-Host "  Wrote $prefsDest"
}

# ── Dispatch preferences (interactive) ──
$dispatchPrefsDest = Join-Path $RulesDir "claude-agent-kit--dispatch-prefs.md"
$keepDispatchPrefs = $false
if (Test-Path $dispatchPrefsDest) {
    $reconf = $null
    $envReconf = [Environment]::GetEnvironmentVariable("DISPATCH_RECONFIGURE")
    if ($envReconf) {
        if ($envReconf -match '^(yes|y|YES|Yes|Y)$') { $reconf = $true }
        elseif ($envReconf -match '^(no|n|NO|No|N)$') { $reconf = $false }
    }
    if ($null -eq $reconf) {
        if ([Environment]::UserInteractive) {
            Write-Host ""
            Write-Host "Existing dispatch preferences found at:"
            Write-Host "  $dispatchPrefsDest"
            $answer = Read-Host "Reconfigure (overwrite)? [y/N]"
            if ($answer -match '^(y|Y|yes|YES|Yes)$') { $reconf = $true } else { $reconf = $false }
        } else {
            $reconf = $false
        }
    }
    if (-not $reconf) {
        $keepDispatchPrefs = $true
        Write-Host "Keeping existing dispatch preferences (edit anytime at $dispatchPrefsDest)."
        $existingManifest = if (Test-Path $Manifest) { Get-Content $Manifest } else { @() }
        if ($existingManifest -notcontains $dispatchPrefsDest) { Add-Content $Manifest $dispatchPrefsDest }
    }
}

if (-not $keepDispatchPrefs) {
    Write-Host ""
    Write-Host "Configuring claude-agent-kit dispatch preferences."
    Write-Host "(set DISPATCH_* environment variables to run non-interactively)"
    Write-Host ""

    $dispPolicy      = Prompt-WithDefault "Execution policy [conservative/preference-only/proactive] (default conservative)"          "DISPATCH_POLICY"      "conservative" '^(conservative|preference-only|proactive)$'
    $dispApproval    = Prompt-WithDefault "Approval mode [ask/auto] (default ask)"                                               "DISPATCH_APPROVAL"    "ask" '^(ask|auto)$'
    $dispGranularity = Prompt-WithDefault "Default approval granularity [per-step/batch/ask] (default ask)"                      "DISPATCH_GRANULARITY" "ask" '^(per-step|batch|ask)$'
    $dispBackend     = Prompt-WithDefault "Default backend [codex/opencode] (default codex)"                                    "DISPATCH_BACKEND"     "codex" '^(codex|opencode)$'
    $dispModel       = Prompt-WithDefault "Default model (codex model id, or opencode provider/model; blank for backend default)" "DISPATCH_MODEL"       ""    $null
    $dispEffort      = Prompt-WithDefault "Default reasoning effort [low/medium/high/xhigh, blank]"                              "DISPATCH_EFFORT"      ""    '^(low|medium|high|xhigh)?$'

    $dtmplUrl = "$RawBase/scripts/claude-agent-kit--dispatch-prefs.md.tmpl"
    $dtmplTmp = New-TemporaryFile
    Invoke-WebRequest -Uri $dtmplUrl -OutFile $dtmplTmp.FullName
    $dtmplContent = Get-Content $dtmplTmp.FullName -Raw
    $dtmplContent = $dtmplContent.Replace("{{POLICY}}",      $dispPolicy)
    $dtmplContent = $dtmplContent.Replace("{{APPROVAL}}",    $dispApproval)
    $dtmplContent = $dtmplContent.Replace("{{GRANULARITY}}", $dispGranularity)
    $dtmplContent = $dtmplContent.Replace("{{BACKEND}}",     $dispBackend)
    $dtmplContent = $dtmplContent.Replace("{{MODEL}}",       $dispModel)
    $dtmplContent = $dtmplContent.Replace("{{EFFORT}}",      $dispEffort)

    Set-Content $dispatchPrefsDest -Value $dtmplContent -NoNewline
    Remove-Item $dtmplTmp.FullName -Force

    $existingManifest = if (Test-Path $Manifest) { Get-Content $Manifest } else { @() }
    if ($existingManifest -notcontains $dispatchPrefsDest) { Add-Content $Manifest $dispatchPrefsDest }
    Write-Host "  Wrote $dispatchPrefsDest"
    Write-Host "  Dispatch execution policy: $dispPolicy"
    Write-Host "  Dispatch approval mode:    $dispApproval"
    Write-Host "  Dispatch default backend:  $dispBackend"
}

# Shared custom-rules ingestion — a function (parity with cak-common.sh's
# ingest_custom_rules on the shell side), called once after both prefs configs.
function Install-CustomRules {
    if (-not ($customRules -and (Test-Path $customRules -PathType Container))) {
        if ($customRules) { Write-Host "  custom rules dir not found: $customRules — skipping" }
        return
    }
    Write-Host "Ingesting custom rules from $customRules ..."
    foreach ($src in Get-ChildItem -Path $customRules -Filter *.md) {
        $base = $src.Name
        $destName = if ($base.StartsWith("claude-agent-kit--")) { $base } else { "claude-agent-kit--$base" }
        $dest = Join-Path $RulesDir $destName

        if ((Test-Path $dest) -and ((Get-Content $dest -TotalCount 1) -match [regex]::Escape("<!-- $Signature -->"))) {
            Write-Host "  refusing to overwrite core kit file: $dest"
            continue
        }

        $firstLine = Get-Content $src.FullName -TotalCount 1
        if ($firstLine -match [regex]::Escape("<!-- $CustomSignature")) {
            Copy-Item $src.FullName -Destination $dest -Force
        } else {
            $body = Get-Content $src.FullName -Raw
            Set-Content $dest -Value "<!-- $CustomSignature`:user -->`n$body" -NoNewline
        }

        $existingManifest = Get-Content $Manifest
        if ($existingManifest -notcontains $dest) {
            Add-Content $Manifest $dest
        }
        Write-Host "  installed $dest"
    }
}
Install-CustomRules

Write-Host ""
Write-Host "To uninstall, run:"
Write-Host "  iwr $RawBase/install.ps1 -OutFile install.ps1; .\install.ps1 -Uninstall"
