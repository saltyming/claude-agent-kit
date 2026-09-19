# Windows twin of remove-legacy-workslate.sh: remove what claude-agent-kit 11.x
# and earlier installed for workslate, the mid-turn messaging MCP server that
# 12.0.0 dropped.
#
# Owns four things and nothing else: the workslate hook entries in
# <ClaudeDir>\settings.json, the <BinDir>\workslate.exe binary, the user-scope
# `workslate` MCP registration, and the per-project workslate.db files. Every
# other hook, server, and file is left alone. install.ps1 runs it on both install
# and uninstall; it is a no-op when nothing is installed.
#
# The hook entries are the part that cannot be skipped: one of them matches every
# tool call, so with the binary gone each call reports a hook error until the
# entry is removed. The JSON edit below does that. The old binary's own
# --uninstall-hooks is the last resort, and only when ClaudeDir is the default:
# that binary always edits %USERPROFILE%\.claude\settings.json, whatever
# ClaudeDir says.
[CmdletBinding()]
param(
    [string]$ClaudeDir = "",
    [string]$BinDir = ""
)
$ErrorActionPreference = "Stop"

# USERPROFILE is unset under PowerShell on macOS / Linux; $HOME is always set.
$userHome = if ($env:USERPROFILE) { $env:USERPROFILE } else { $HOME }
$defaultClaudeDir = Join-Path $userHome ".claude"
if (-not $ClaudeDir) { $ClaudeDir = $defaultClaudeDir }
if (-not $BinDir) { $BinDir = Join-Path (Join-Path $userHome ".local") "bin" }

$settings = Join-Path $ClaudeDir "settings.json"
$workslateExe = Join-Path $BinDir "workslate.exe"

function Test-WorkslateHooks {
    if (-not (Test-Path $settings -PathType Leaf)) { return $false }
    $raw = Get-Content $settings -Raw
    if ([string]::IsNullOrEmpty($raw)) { return $false }
    return ($raw -match 'workslate[^"]*--hook=' -or $raw.Contains('[workslate-task-verify]'))
}

function Test-OurHandler($handler) {
    if ($null -eq $handler -or $handler -isnot [System.Management.Automation.PSCustomObject]) { return $false }
    $command = $handler.PSObject.Properties['command']
    if ($command -and $command.Value -is [string] -and
        $command.Value.Contains('workslate') -and $command.Value.Contains('--hook=')) {
        return $true
    }
    $type = $handler.PSObject.Properties['type']
    $prompt = $handler.PSObject.Properties['prompt']
    return ($type -and $type.Value -eq 'agent' -and $prompt -and $prompt.Value -is [string] -and
        $prompt.Value.StartsWith('[workslate-task-verify]'))
}

# Returns $true when the file was rewritten. Throws on unparseable JSON; the
# caller turns that into the manual-cleanup warning.
function Remove-WorkslateHooks {
    $data = Get-Content $settings -Raw | ConvertFrom-Json
    $hooksProp = $data.PSObject.Properties['hooks']
    if (-not $hooksProp -or $hooksProp.Value -isnot [System.Management.Automation.PSCustomObject]) { return $false }
    $hooks = $hooksProp.Value
    $changed = $false
    foreach ($eventName in @($hooks.PSObject.Properties.Name)) {
        $groups = $hooks.$eventName
        if ($groups -isnot [System.Array]) { continue }
        $kept = New-Object System.Collections.ArrayList
        foreach ($group in $groups) {
            $handlersProp = if ($group -is [System.Management.Automation.PSCustomObject]) { $group.PSObject.Properties['hooks'] } else { $null }
            if ($handlersProp -and $handlersProp.Value -is [System.Array]) {
                $remaining = @($handlersProp.Value | Where-Object { -not (Test-OurHandler $_) })
                if ($remaining.Count -ne $handlersProp.Value.Count) {
                    $changed = $true
                    if ($remaining.Count -eq 0) { continue }
                    $group.hooks = $remaining
                }
            }
            [void]$kept.Add($group)
        }
        if ($kept.Count -gt 0) {
            $hooks.$eventName = @($kept.ToArray())
        } elseif ($groups.Count -gt 0) {
            $hooks.PSObject.Properties.Remove($eventName)
        }
    }
    if (-not $changed) { return $false }

    $stamp = [DateTime]::UtcNow.ToString("yyyyMMddTHHmmssZ")
    Copy-Item $settings -Destination "$settings.bak-$stamp" -Force
    # -Depth: the default of 2 would flatten the hook groups into strings.
    $json = $data | ConvertTo-Json -Depth 100
    $tmp = "$settings.tmp.$PID"
    [System.IO.File]::WriteAllText($tmp, $json + "`n", (New-Object System.Text.UTF8Encoding($false)))
    Move-Item $tmp -Destination $settings -Force
    return $true
}

if (Test-WorkslateHooks) {
    try { [void](Remove-WorkslateHooks) } catch {}
    if ((Test-WorkslateHooks) -and (Test-Path $workslateExe -PathType Leaf) -and ($ClaudeDir -eq $defaultClaudeDir)) {
        try { & $workslateExe --uninstall-hooks 2>$null | Out-Null } catch {}
    }
    if (Test-WorkslateHooks) {
        Write-Warning "workslate hooks are still present in $settings and could not be removed automatically (the file must be valid JSON). Delete every hook entry whose command contains both 'workslate' and '--hook=', or each tool call will report a hook error."
    } else {
        Write-Host "  removed workslate hooks from $settings"
    }
}

if (Test-Path $workslateExe -PathType Leaf) {
    Remove-Item $workslateExe -Force
    Write-Host "  removed $workslateExe"
}

# `claude mcp` edits the real user config whatever ClaudeDir says, so it is
# skipped for a non-default ClaudeDir (a scratch or test install).
if (($ClaudeDir -eq $defaultClaudeDir) -and (Get-Command claude -ErrorAction SilentlyContinue)) {
    try {
        claude mcp remove workslate -s user 2>$null | Out-Null
        if ($LASTEXITCODE -eq 0) { Write-Host "  workslate MCP server unregistered." }
    } catch {}
}

$projects = Join-Path $ClaudeDir "projects"
if (Test-Path $projects -PathType Container) {
    foreach ($dir in Get-ChildItem -Path $projects -Directory) {
        $wsDir = Join-Path $dir.FullName "workslate"
        if (-not (Test-Path $wsDir -PathType Container)) { continue }
        foreach ($name in @("workslate.db", "workslate.db-wal", "workslate.db-shm")) {
            $f = Join-Path $wsDir $name
            if (Test-Path $f -PathType Leaf) { Remove-Item $f -Force }
        }
        if (-not (Get-ChildItem -Path $wsDir -Force)) { Remove-Item $wsDir -Force }
    }
}
