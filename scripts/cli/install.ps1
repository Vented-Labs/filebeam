param(
    [string] $InstallDir = (Join-Path $HOME '.filebeam'),
    [string] $Version = 'latest'
)

$ErrorActionPreference = 'Stop'
$BaseUrl = if ($env:BEAM_RELEASE_BASE_URL) { $env:BEAM_RELEASE_BASE_URL.TrimEnd('/') } else { 'https://releases.filebeam.io/cli' }
$ReleasePublicKey = '__BEAM_RELEASE_PUBLIC_KEY__'
$MaximumCatalogBytes = 1MB
$MaximumArchiveBytes = 100MB

Add-Type -AssemblyName System.Net.Http

if ($ReleasePublicKey.StartsWith('__')) { throw 'This installer has no embedded release verification key.' }
if ([string]::IsNullOrWhiteSpace($InstallDir) -or $InstallDir.IndexOfAny(@("`r", "`n")) -ge 0) {
    throw 'The install directory is invalid.'
}
if ($Version -ne 'latest' -and $Version -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
    throw 'Version must be X.Y.Z.'
}
if (-not [Environment]::Is64BitOperatingSystem -or $env:PROCESSOR_ARCHITECTURE -notin @('AMD64', 'x86')) {
    throw "Unsupported Windows architecture: $env:PROCESSOR_ARCHITECTURE"
}

function Get-LimitedBytes([string] $Uri, [long] $Maximum) {
    $client = [System.Net.Http.HttpClient]::new()
    $response = $null
    $stream = $null
    $output = $null
    try {
        $response = $client.GetAsync($Uri, [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
        $response.EnsureSuccessStatusCode() | Out-Null
        if ($response.Content.Headers.ContentLength -and $response.Content.Headers.ContentLength -gt $Maximum) {
            throw 'Download exceeds its allowed size.'
        }
        $stream = $response.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
        $output = [IO.MemoryStream]::new()
        $buffer = [byte[]]::new(81920)
        while ($true) {
            $read = $stream.ReadAsync($buffer, 0, $buffer.Length).GetAwaiter().GetResult()
            if ($read -eq 0) { break }
            if ($output.Length + $read -gt $Maximum) { throw 'Download exceeds its allowed size.' }
            $output.Write($buffer, 0, $read)
        }
        return $output.ToArray()
    } finally {
        if ($output) { $output.Dispose() }
        if ($stream) { $stream.Dispose() }
        if ($response) { $response.Dispose() }
        $client.Dispose()
    }
}

function Get-OpenSsl {
    $command = Get-Command openssl.exe -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    foreach ($path in @(
        (Join-Path $env:ProgramFiles 'Git\usr\bin\openssl.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Git\usr\bin\openssl.exe')
    )) {
        if (Test-Path $path) { return $path }
    }
    throw 'OpenSSL is required to verify Filebeam releases. Install Git for Windows and retry.'
}

$temporary = Join-Path ([IO.Path]::GetTempPath()) ("beam-install-{0}" -f [Guid]::NewGuid())
New-Item -ItemType Directory -Path $temporary | Out-Null
try {
    $envelopeBytes = Get-LimitedBytes "$BaseUrl/index.json" $MaximumCatalogBytes
    $envelope = [Text.Encoding]::UTF8.GetString($envelopeBytes) | ConvertFrom-Json
    $payload = [Convert]::FromBase64String($envelope.signed)
    $signature = [Convert]::FromBase64String($envelope.signature)
    if ($signature.Length -ne 64) { throw 'Release catalog signature is invalid.' }
    $key = [Convert]::FromBase64String($ReleasePublicKey)
    if ($key.Length -ne 32) { throw 'The embedded release verification key is invalid.' }
    [IO.File]::WriteAllBytes((Join-Path $temporary 'index.json'), $payload)
    [IO.File]::WriteAllBytes((Join-Path $temporary 'index.sig'), $signature)
    $der = [byte[]] (0x30,0x2a,0x30,0x05,0x06,0x03,0x2b,0x65,0x70,0x03,0x21,0x00) + $key
    [IO.File]::WriteAllBytes((Join-Path $temporary 'public.der'), $der)
    $openssl = Get-OpenSsl
    & $openssl pkey -pubin -inform DER -in (Join-Path $temporary 'public.der') -out (Join-Path $temporary 'public.pem') 2>$null
    if ($LASTEXITCODE -ne 0) { throw 'The embedded release verification key is invalid.' }
    & $openssl pkeyutl -verify -pubin -inkey (Join-Path $temporary 'public.pem') -rawin -in (Join-Path $temporary 'index.json') -sigfile (Join-Path $temporary 'index.sig') 2>$null
    if ($LASTEXITCODE -ne 0) { throw 'Release catalog signature verification failed.' }

    $catalog = [Text.Encoding]::UTF8.GetString($payload) | ConvertFrom-Json
    if ($catalog.schema -ne 1 -or [DateTimeOffset]::Parse($catalog.expires_at) -le [DateTimeOffset]::UtcNow) {
        throw 'The signed release catalog has expired or is unsupported.'
    }
    $release = if ($Version -eq 'latest') { $catalog.releases | Select-Object -First 1 } else {
        $catalog.releases | Where-Object version -eq $Version | Select-Object -First 1
    }
    if (-not $release) { throw 'The requested release does not exist.' }
    $Version = $release.version
    $asset = $release.assets | Where-Object { $_.os -eq 'windows' -and $_.architecture -eq 'x86_64' } | Select-Object -First 1
    $archive = "beam-v$Version-windows-x86_64.zip"
    if (-not $asset -or $asset.path -ne "versions/v$Version/$archive" -or $asset.size -le 0 -or $asset.size -gt $MaximumArchiveBytes) {
        throw 'The signed catalog does not contain the expected archive.'
    }
    $archiveBytes = Get-LimitedBytes "$BaseUrl/$($asset.path)" $asset.size
    if ($archiveBytes.LongLength -ne $asset.size) { throw 'Archive size mismatch.' }
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try { $digest = ([BitConverter]::ToString($sha256.ComputeHash($archiveBytes))).Replace('-', '').ToLowerInvariant() }
    finally { $sha256.Dispose() }
    if ($digest -ne $asset.sha256.ToLowerInvariant()) { throw 'Archive checksum mismatch.' }
    $archivePath = Join-Path $temporary $archive
    [IO.File]::WriteAllBytes($archivePath, $archiveBytes)
    Expand-Archive -LiteralPath $archivePath -DestinationPath $temporary
    $source = Join-Path $temporary 'beam\beam.exe'
    if (-not (Test-Path $source -PathType Leaf)) { throw 'Archive does not contain beam.exe.' }

    $bin = Join-Path $InstallDir 'bin'
    New-Item -ItemType Directory -Force -Path $bin, (Join-Path $InstallDir 'cache') | Out-Null
    $config = Join-Path $InstallDir 'config.toml'
    if (-not (Test-Path $config)) { New-Item -ItemType File -Path $config | Out-Null }
    $destination = Join-Path $bin 'beam.exe'
    $candidate = Join-Path $bin (".beam-{0}.exe" -f $PID)
    $backup = Join-Path $bin 'beam.previous.exe'
    Copy-Item -LiteralPath $source -Destination $candidate
    try {
        if (Test-Path $destination -PathType Leaf) {
            Remove-Item -Force -ErrorAction SilentlyContinue $backup
            [IO.File]::Replace($candidate, $destination, $backup)
        } else {
            [IO.File]::Move($candidate, $destination)
        }
    } finally {
        Remove-Item -Force -ErrorAction SilentlyContinue $candidate
    }
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $parts = @($userPath -split ';' | Where-Object { $_ })
    if ($parts -notcontains $bin) {
        [Environment]::SetEnvironmentVariable('Path', (($parts + $bin) -join ';'), 'User')
    }
    if (($env:Path -split ';') -notcontains $bin) { $env:Path = "$bin;$env:Path" }
    Write-Output "Installed beam $Version to $destination"
} finally {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $temporary
}
