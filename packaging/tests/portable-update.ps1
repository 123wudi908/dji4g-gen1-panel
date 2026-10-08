[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$testRoot = Join-Path $repoRoot ('target/portable-update-test-' + [guid]::NewGuid().ToString('N'))
$targetRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot 'target')) + [IO.Path]::DirectorySeparatorChar
if (![IO.Path]::GetFullPath($testRoot).StartsWith($targetRoot,[StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe test path' }
New-Item -ItemType Directory -Path (Join-Path $testRoot 'binaries') -Force | Out-Null
try {
    foreach ($name in @('dji4g-panel.exe','dji4g-helper.exe','dji4g-updater.exe')) {
        [IO.File]::WriteAllBytes((Join-Path $testRoot "binaries/$name"),[Text.Encoding]::UTF8.GetBytes("fixture:$name"))
    }
    $result = & (Join-Path $repoRoot 'packaging/scripts/build-portable.ps1') -OutputDirectory (Join-Path $testRoot 'dist') -BinaryDirectory (Join-Path $testRoot 'binaries') | ConvertFrom-Json
    if ($result.status -ne 'verified') { throw 'Portable builder verification failed' }
    $expected = @('dji4g-panel.exe','dji4g-helper.exe','dji4g-updater.exe','使用说明.txt','LICENSE-MIT','LICENSE-APACHE','THIRD-PARTY-NOTICES.txt','portable-manifest.json')
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($result.archive)
    try {
        if ($zip.Entries.Count -ne $expected.Count) { throw 'Portable payload count mismatch' }
        foreach ($name in $expected) { if (!($zip.Entries | Where-Object FullName -CEQ $name)) { throw "Missing portable payload: $name" } }
        $entry = $zip.Entries | Where-Object FullName -CEQ 'portable-manifest.json'
        $reader = [IO.StreamReader]::new($entry.Open())
        try { $manifest = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
        if ($manifest.Count -ne 7) { throw 'Manifest must cover all seven payload files' }
        foreach ($item in $manifest) {
            if ($item.name -notin $expected -or $item.name -eq 'portable-manifest.json') { throw 'Unexpected manifest path' }
            $payload = $zip.Entries | Where-Object FullName -CEQ $item.name
            $stream = $payload.Open()
            $hasher = [Security.Cryptography.SHA256]::Create()
            try { $hash = [Convert]::ToHexString($hasher.ComputeHash($stream)) } finally { $stream.Dispose(); $hasher.Dispose() }
            if ($hash -cne $item.sha256) { throw 'Payload manifest SHA-256 mismatch' }
        }
    } finally { $zip.Dispose() }
    $sidecar = (Get-Content -LiteralPath ($result.archive + '.sha256') -Raw).Trim()
    $actual = (Get-FileHash -LiteralPath $result.archive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($sidecar -cne "$actual  dji4g-panel-windows-x64-portable.zip") { throw 'Release ZIP sidecar mismatch' }
    Remove-Item -LiteralPath (Join-Path $testRoot 'binaries/dji4g-updater.exe')
    $rejected = $false
    try { & (Join-Path $repoRoot 'packaging/scripts/build-portable.ps1') -OutputDirectory (Join-Path $testRoot 'missing') -BinaryDirectory (Join-Path $testRoot 'binaries') | Out-Null } catch { $rejected = $true }
    if (!$rejected) { throw 'Builder accepted missing updater' }
    '{"status":"passed","test":"portable-update","checks":12}'
} finally {
    if (Test-Path -LiteralPath $testRoot) { Remove-Item -LiteralPath $testRoot -Recurse -Force }
}
