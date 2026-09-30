# install.ps1 - installs, reconfigures or removes claude-agent-kit on Windows.
#
# This script obtains two things and runs the installer; it implements no
# installer step itself:
#   1. the kit payload: the dist\ folder beside this script when it has one,
#      otherwise the archive of the kit repository (saltyming/claude-agent-kit) at --ref;
#   2. slate-setup: built with cargo from --slate-dir when --binaries build is
#      given, otherwise the prebuilt binary for this platform from the slate
#      release v0.9.1, verified against the release's checksums.txt.
#
# Usage: .\install.ps1 [install|configure|uninstall] [options]
# The command is optional and defaults to install, also when the first argument
# starts with a dash. Every option is passed to slate-setup, except --ref, which
# selects the kit archive; --uninstall and --skip-mcp are kept as aliases of the
# uninstall command and of --binaries skip; -Uninstall, -SkipMcp and
# -DispatchRoots <paths> from the earlier script work too, and $env:DISPATCH_ROOTS
# stands for --roots when none is given. slate-setup reads its questions from
# the console itself, so nothing here redirects standard input.
#
# One-liner (arguments after the script block go to the script):
#   & ([scriptblock]::Create((Invoke-RestMethod https://raw.githubusercontent.com/saltyming/claude-agent-kit/main/install.ps1))) install

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
try { [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12 -bor [Net.ServicePointManager]::SecurityProtocol } catch { }

$KitName = 'claude-agent-kit'
$KitRepo = 'saltyming/claude-agent-kit'
$SlateRepo = if ($env:SLATE_RELEASE_REPO) { $env:SLATE_RELEASE_REPO } else { 'saltyming/slate-agent-kit' }
$SlateVersion = '0.9.1'
$ReleaseHost = if ($env:SLATE_RELEASE_BASE_URL) { $env:SLATE_RELEASE_BASE_URL.TrimEnd('/') } else { 'https://github.com' }
$KitArchiveHost = if ($env:KIT_ARCHIVE_BASE_URL) { $env:KIT_ARCHIVE_BASE_URL.TrimEnd('/') } else { 'https://github.com' }

function Show-Usage {
    @"
Usage: .\install.ps1 [install|configure|uninstall] [options]

  install      full install or reinstall of $KitName (default)
  configure    prefs, custom rules, native configuration and server registration
  uninstall    reverse what install recorded

Options are passed to slate-setup. The common ones:
  --home DIR            harness home (default: the harness's own folder)
  --bin-dir DIR         where binaries go (default: %USERPROFILE%\.local\bin)
  --binaries MODE       prebuilt (default), build, or skip
  --slate-dir DIR       slate checkout to build from (with --binaries build)
  --roots PATHS         workspace roots for dispatch and palette (separated by ;)
  --set KEY=VALUE       set a prefs value, for example aside.level=auto
  --custom-rules DIR    folder of your own *.md rule files
  --payload DIR         install from this dist folder
  --yes                 ask nothing; take current or default values
  --dry-run             print the summary and stop
Only for this script:
  --ref REF             kit branch, tag or commit to download (default: main)
  --uninstall           same as the uninstall command
  --skip-mcp            same as --binaries skip
  -Uninstall, -SkipMcp, -DispatchRoots PATHS    the switches of the earlier script
Environment:
  DISPATCH_ROOTS        same as --roots when --roots is not given
"@
}

function Get-Download([string]$Url, [string]$Dest) {
    # Returns $true on success and $false when the server answers 404.
    try {
        Invoke-WebRequest -Uri $Url -OutFile $Dest -UseBasicParsing
        return $true
    } catch {
        $status = $null
        if ($_.Exception.Response) { $status = [int]$_.Exception.Response.StatusCode }
        if ($status -eq 404) { return $false }
        throw "cannot download ${Url}: $($_.Exception.Message)"
    }
}

# --- arguments ---------------------------------------------------------------
$rest = New-Object System.Collections.Generic.List[string]
foreach ($a in $args) { $rest.Add([string]$a) }
$cmd = 'install'
if ($rest.Count -gt 0 -and @('install', 'configure', 'uninstall') -contains $rest[0]) {
    $cmd = $rest[0]
    $rest.RemoveAt(0)
}
$ref = 'main'
$binaries = 'prebuilt'
$slateDir = $null
$payloadGiven = $null
$rootsGiven = $false
$passthrough = New-Object System.Collections.Generic.List[string]
$i = 0
while ($i -lt $rest.Count) {
    $a = $rest[$i]
    switch -Regex ($a) {
        '^(-h|--help)$' { Show-Usage; exit 0 }
        '^--ref$' { $i++; if ($i -ge $rest.Count) { throw '--ref needs a value' }; $ref = $rest[$i] }
        '^--ref=(.*)$' { $ref = $Matches[1] }
        # Aliases for the flags earlier releases had.
        '^(--uninstall|-Uninstall)$' { $cmd = 'uninstall' }
        '^(--skip-mcp|-SkipMcp)$' { $binaries = 'skip'; $passthrough.Add('--binaries'); $passthrough.Add('skip') }
        '^-DispatchRoots$' {
            $i++
            if ($i -ge $rest.Count) { throw '-DispatchRoots needs a value' }
            $rootsGiven = $true
            $passthrough.Add('--roots')
            $passthrough.Add($rest[$i])
        }
        '^-DispatchRoots:(.*)$' { $rootsGiven = $true; $passthrough.Add('--roots'); $passthrough.Add($Matches[1]) }
        '^--roots$' {
            $i++
            if ($i -ge $rest.Count) { throw '--roots needs a value' }
            $rootsGiven = $true
            $passthrough.Add($a)
            $passthrough.Add($rest[$i])
        }
        '^--roots=(.*)$' { $rootsGiven = $true; $passthrough.Add($a) }
        '^--(binaries|slate-dir|payload)$' {
            $name = $Matches[1]
            $i++
            if ($i -ge $rest.Count) { throw "$a needs a value" }
            $val = $rest[$i]
            if ($name -eq 'binaries') { $binaries = $val }
            elseif ($name -eq 'slate-dir') { $slateDir = $val }
            else { $payloadGiven = $val }
            $passthrough.Add($a)
            $passthrough.Add($val)
        }
        '^--binaries=(.*)$' { $binaries = $Matches[1]; $passthrough.Add($a) }
        '^--slate-dir=(.*)$' { $slateDir = $Matches[1]; $passthrough.Add($a) }
        '^--payload=(.*)$' { $payloadGiven = $Matches[1]; $passthrough.Add($a) }
        default { $passthrough.Add($a) }
    }
    $i++
}

# The earlier installers read DISPATCH_ROOTS; it still seeds --roots.
if (-not $rootsGiven -and $env:DISPATCH_ROOTS) {
    $passthrough.Add('--roots')
    $passthrough.Add($env:DISPATCH_ROOTS)
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) ('slate-setup-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
$exitCode = 1
try {
    # --- the kit payload -------------------------------------------------------
    $here = if ($PSScriptRoot) { $PSScriptRoot } else { (Get-Location).Path }
    if ($payloadGiven) {
        $payload = $payloadGiven
    } elseif (Test-Path -LiteralPath (Join-Path $here 'dist\kit.toml')) {
        $payload = Join-Path $here 'dist'
    } else {
        Write-Host "Downloading $KitName ($ref)..."
        $zip = Join-Path $tmp 'kit.zip'
        if (-not (Get-Download "$KitArchiveHost/$KitRepo/archive/$ref.zip" $zip)) {
            throw "cannot download the $KitName archive for '$ref'"
        }
        Expand-Archive -LiteralPath $zip -DestinationPath (Join-Path $tmp 'kit')
        $payload = $null
        foreach ($d in Get-ChildItem -LiteralPath (Join-Path $tmp 'kit') -Directory) {
            $candidate = Join-Path $d.FullName 'dist'
            if (Test-Path -LiteralPath (Join-Path $candidate 'kit.toml')) { $payload = $candidate; break }
        }
        if (-not $payload) { throw "the archive for '$ref' has no dist\kit.toml" }
    }

    # --- slate-setup -------------------------------------------------------------
    if ($binaries -eq 'build') {
        if (-not $slateDir) { throw '--binaries build needs --slate-dir <slate checkout>' }
        if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'cargo is required for --binaries build' }
        Write-Host "Building slate-setup in $slateDir..."
        & cargo build --release -p slate-setup --manifest-path (Join-Path $slateDir 'Cargo.toml')
        if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
        $targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $slateDir 'target' }
        $setup = Join-Path $targetDir 'release\slate-setup.exe'
    } else {
        if ($env:SLATE_PLATFORM) {
            $platform = $env:SLATE_PLATFORM
        } else {
            $arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
            switch ($arch) {
                'ARM64' { $cpu = 'aarch64' }
                'AMD64' { $cpu = 'x86_64' }
                default { throw "unsupported architecture $arch; use --binaries build --slate-dir <slate checkout>" }
            }
            $platform = "$cpu-pc-windows-msvc"
        }
        $asset = "slate-setup-$platform.zip"
        $base = "$ReleaseHost/$SlateRepo/releases/download/v$SlateVersion"
        $latest = "$ReleaseHost/$SlateRepo/releases/latest/download"
        Write-Host "Downloading slate-setup ($platform)..."
        $archive = Join-Path $tmp $asset
        if (-not (Get-Download "$base/$asset" $archive)) {
            Write-Host "slate release v$SlateVersion has no $asset; using the latest release."
            $base = $latest
            if (-not (Get-Download "$base/$asset" $archive)) {
                throw "cannot download $asset; use --binaries build --slate-dir <slate checkout>"
            }
        }
        $sums = Join-Path $tmp 'checksums.txt'
        if (Get-Download "$base/checksums.txt" $sums) {
            $line = Get-Content -LiteralPath $sums | Where-Object { $_ -match "^\s*([0-9a-fA-F]{64})\s+\*?$([regex]::Escape($asset))\s*$" } | Select-Object -First 1
            if (-not $line) {
                Write-Warning "checksums.txt has no entry for $asset; it is not verified."
            } else {
                $expected = ($line -split '\s+')[0].ToLowerInvariant()
                $actual = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
                if ($expected -ne $actual) {
                    throw "checksum mismatch for $asset (expected $expected, got $actual)"
                }
            }
        } else {
            Write-Warning "the release has no checksums.txt; $asset is not verified."
        }
        Expand-Archive -LiteralPath $archive -DestinationPath $tmp -Force
        $setup = Join-Path $tmp 'slate-setup.exe'
    }

    # --- run -----------------------------------------------------------------------
    $runArgs = New-Object System.Collections.Generic.List[string]
    $runArgs.Add($cmd)
    if (-not $payloadGiven) { $runArgs.Add('--payload'); $runArgs.Add($payload) }
    foreach ($p in $passthrough) { $runArgs.Add($p) }
    & $setup @runArgs
    $exitCode = $LASTEXITCODE
} catch {
    [Console]::Error.WriteLine("error: $($_.Exception.Message)")
    $exitCode = 1
} finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
exit $exitCode
