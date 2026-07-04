# install.ps1 — Windows installer for claude-agent-kit
# Usage:
#   iwr https://raw.githubusercontent.com/saltyming/claude-agent-kit/main/install.ps1 | iex
#   iwr https://raw.githubusercontent.com/saltyming/claude-agent-kit/main/install.ps1 -OutFile install.ps1; .\install.ps1 -Uninstall

param([switch]$Uninstall, [string]$DispatchRoots = $env:DISPATCH_ROOTS)

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

# Register MCP servers — absolute paths + per-server env, mirroring
# install-mcp.sh's configure_claude (bare-name registration fails when
# ~/.local\bin is not on PATH; aside needs ASIDE_HARNESS, dispatch needs
# SLATE_AGENT_STATE_HOME so per-project task history is not orphaned).
if (Get-Command claude -ErrorAction SilentlyContinue) {
    foreach ($srv in $Binaries) {
        $srvExe = Join-Path $BinDir "$srv.exe"
        $envArgs = @()
        if ($srv -eq "aside") {
            $envArgs = @("-e", "ASIDE_HARNESS=claude")
        } elseif ($srv -eq "dispatch") {
            $envArgs = @("-e", "SLATE_AGENT_STATE_HOME=$ClaudeDir")
            if ($DispatchRoots) { $envArgs += @("-e", "DISPATCH_EXTRA_ROOTS=$DispatchRoots") }
        }
        Write-Host "Registering $srv MCP server..."
        claude mcp remove $srv -s user 2>$null | Out-Null
        try {
            claude mcp add $srv -s user --transport stdio @envArgs -- $srvExe 2>$null
            Write-Host "  $srv registered."
        } catch {
            Write-Host "  $srv registration failed. Add manually: claude mcp add $srv -s user --transport stdio @envArgs -- $srvExe"
        }
    }
} else {
    Write-Host "Claude Code CLI not found. Register MCP servers manually, e.g.:"
    Write-Host "  claude mcp add aside -s user --transport stdio -e ASIDE_HARNESS=claude -- $(Join-Path $BinDir 'aside.exe')"
    Write-Host "  claude mcp add dispatch -s user --transport stdio -e SLATE_AGENT_STATE_HOME=$ClaudeDir -- $(Join-Path $BinDir 'dispatch.exe')"
}

# ── aside + dispatch preferences (the shared configure-prefs.ps1) ──
# The SAME generator every kit uses — interactive-first, asks before overwrite,
# injection-safe. slate's POSIX configure-prefs.sh cannot run on Windows, so it
# is fetched alongside the rules and invoked here.
$prefsTmp = Join-Path $env:TEMP ("cak-prefs-" + [System.Guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $prefsTmp | Out-Null
Invoke-WebRequest -Uri "$RawBase/scripts/configure-prefs.ps1" -OutFile (Join-Path $prefsTmp "configure-prefs.ps1")
foreach ($t in @("aside", "dispatch")) {
    Invoke-WebRequest -Uri "$RawBase/scripts/claude-agent-kit--$t-prefs.md.tmpl" -OutFile (Join-Path $prefsTmp "claude-agent-kit--$t-prefs.md.tmpl")
}
& (Join-Path $prefsTmp "configure-prefs.ps1") -RulesDir $RulesDir -Prefix "claude-agent-kit" -Manifest $Manifest
Remove-Item $prefsTmp -Recurse -Force -ErrorAction SilentlyContinue

# Custom-rules directory (claude-specific; ingested by Install-CustomRules below).
$customRules = $null
if ([Environment]::GetEnvironmentVariable("CUSTOM_RULES_DIR")) {
    $customRules = [Environment]::GetEnvironmentVariable("CUSTOM_RULES_DIR")
} elseif ([Environment]::GetEnvironmentVariable("ASIDE_CUSTOM_RULES_DIR")) {
    $customRules = [Environment]::GetEnvironmentVariable("ASIDE_CUSTOM_RULES_DIR")
} elseif ([Environment]::UserInteractive) {
    $ans = Read-Host "Path to a directory of your own custom rule files (blank to skip)"
    if (-not [string]::IsNullOrEmpty($ans)) { $customRules = $ans }
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
