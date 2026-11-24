# Stream Preflight installer for Windows.
#   irm https://github.com/Matt-B-007/Stream-Preflight/releases/latest/download/install.ps1 | iex
# Downloads the newest release, checks it against SHA256SUMS.txt, installs it
# for this user (no admin), adds a Start menu shortcut and an entry in
# Settings > Apps, then opens it. After that, Stream Preflight updates itself.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Repo = 'Matt-B-007/Stream-Preflight'
$Base = "https://github.com/$Repo/releases/latest/download"
$File = 'stream-preflight-windows-x86_64.exe'

if (-not [Environment]::Is64BitOperatingSystem) { throw 'Stream Preflight needs 64-bit Windows.' }

$Tmp = Join-Path ([IO.Path]::GetTempPath()) ("stream-preflight-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $Tmp | Out-Null
try {
    Write-Host 'Downloading Stream Preflight...'
    Invoke-WebRequest -UseBasicParsing -Uri "$Base/$File" -OutFile (Join-Path $Tmp $File)
    Invoke-WebRequest -UseBasicParsing -Uri "$Base/SHA256SUMS.txt" -OutFile (Join-Path $Tmp 'SHA256SUMS.txt')

    Write-Host 'Checking the download...'
    $line = Get-Content (Join-Path $Tmp 'SHA256SUMS.txt') | Where-Object { $_ -match "\s\*?$([regex]::Escape($File))$" } | Select-Object -First 1
    if (-not $line) { throw "SHA256SUMS.txt doesn't list $File." }
    $want = ($line -split '\s+')[0].ToLower()
    $got = (Get-FileHash -Algorithm SHA256 (Join-Path $Tmp $File)).Hash.ToLower()
    if ($want -ne $got) { throw "The download didn't match its checksum. Nothing was installed." }

    # Copies itself to %LOCALAPPDATA%\Programs\Stream Preflight, adds the shortcut and Apps entry.
    $p = Start-Process -FilePath (Join-Path $Tmp $File) -ArgumentList '--install' -Wait -PassThru -WindowStyle Hidden
    if ($p.ExitCode -ne 0) { throw 'Install failed.' }

    $Installed = Join-Path $env:LOCALAPPDATA 'Programs\Stream Preflight\stream-preflight.exe'
    Write-Host ''
    Write-Host 'Done. Stream Preflight is in your Start menu. Opening it now...'
    Write-Host 'It checks for updates by itself; click Update now when it offers one.'
    Start-Process $Installed
}
finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}
