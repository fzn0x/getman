# Installs the latest Getman release for Windows, after checking its SHA-256
# checksum against the release's SHA256SUMS, and adds it to your user PATH.
#
#   irm https://fzn0x.github.io/getman/install.ps1 | iex
#
# $env:GETMAN_VERSION = 'v0.1.0' installs a given release instead of the latest.
# $env:GETMAN_INSTALL_DIR chooses the folder; the default is
# %LOCALAPPDATA%\Programs\getman.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$repo = 'fzn0x/getman'
$version = if ($env:GETMAN_VERSION) { $env:GETMAN_VERSION } else { 'latest' }
$dir = if ($env:GETMAN_INSTALL_DIR) { $env:GETMAN_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\getman' }

# ARM64 Windows 11 runs the x64 build.
if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64' -and $env:PROCESSOR_ARCHITECTURE -ne 'ARM64') {
    throw "There is no Getman release for $($env:PROCESSOR_ARCHITECTURE) Windows. Build it from source: https://github.com/$repo#building-from-source"
}
$target = 'x86_64-pc-windows-msvc'

if ($version -eq 'latest') {
    $release = Invoke-RestMethod -UseBasicParsing -Uri "https://api.github.com/repos/$repo/releases/latest" -Headers @{ 'User-Agent' = 'getman-install' }
    $version = $release.tag_name
}

$name = "getman-$version-$target"
$base = "https://github.com/$repo/releases/download/$version"
$tmp = Join-Path ([IO.Path]::GetTempPath()) ('getman-' + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null

try {
    Write-Host "Downloading Getman $version for $target"
    Invoke-WebRequest -UseBasicParsing -Uri "$base/$name.zip" -OutFile (Join-Path $tmp "$name.zip")
    Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS" -OutFile (Join-Path $tmp 'SHA256SUMS')

    $pattern = '\s\*?' + [regex]::Escape("$name.zip") + '$'
    $line = Get-Content (Join-Path $tmp 'SHA256SUMS') | Where-Object { $_ -match $pattern } | Select-Object -First 1
    if (-not $line) { throw "$name.zip is not listed in SHA256SUMS." }
    $expected = ($line -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 -Path (Join-Path $tmp "$name.zip")).Hash.ToLowerInvariant()
    if ($expected -ne $actual) {
        throw 'The checksum does not match, so nothing was installed. The download may be damaged or tampered with.'
    }

    Expand-Archive -Path (Join-Path $tmp "$name.zip") -DestinationPath $tmp -Force
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $exe = Join-Path $dir 'getman.exe'
    # Windows cannot overwrite a running program, but it can rename one.
    if (Test-Path $exe) {
        Remove-Item "$exe.old" -Force -ErrorAction SilentlyContinue
        Move-Item $exe "$exe.old" -Force
    }
    Copy-Item (Join-Path $tmp "$name\getman.exe") $exe -Force
    Remove-Item "$exe.old" -Force -ErrorAction SilentlyContinue
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

Write-Host "Installed $exe"
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($userPath -split ';') -notcontains $dir) {
    $newPath = if ($userPath) { "$userPath;$dir" } else { $dir }
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    $env:Path = "$env:Path;$dir"
    Write-Host "Added $dir to your user PATH. New terminals will find getman; run: getman --version"
}
else {
    Write-Host 'Run: getman --version'
}
