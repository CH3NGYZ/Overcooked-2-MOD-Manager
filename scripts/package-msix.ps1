#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $BinaryPath,
    [Parameter(Mandatory)] [string] $Version,
    [Parameter(Mandatory)] [string] $OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false

if (-not $IsWindows) {
    throw 'MakeAppx packaging requires Windows.'
}
if ($Version -cnotmatch '\A[1-9][0-9]{0,4}\.(0|[1-9][0-9]{0,4})\.(0|[1-9][0-9]{0,4})\z' -or
    ($Version.Split('.') | Where-Object { [int] $_ -gt 65535 })) {
    throw "Expected a release version X.Y.Z (nonzero major, each part <= 65535), got '$Version'."
}

$packageVersion = "$Version.0"
if (-not [IO.Path]::IsPathFullyQualified($BinaryPath)) {
    throw 'BinaryPath must be the absolute path to the built OC2ModManager.exe.'
}
$binary = Get-Item -LiteralPath $BinaryPath
if ($binary -isnot [IO.FileInfo] -or $binary.Name -ine 'OC2ModManager.exe') {
    throw "Expected the built OC2ModManager.exe, got '$BinaryPath'."
}
$stream = $binary.OpenRead()
try {
    $pe = [System.Reflection.PortableExecutable.PEReader]::new($stream)
    try {
        if ($pe.PEHeaders.CoffHeader.Machine -ne [System.Reflection.PortableExecutable.Machine]::Amd64 -or
            $pe.PEHeaders.PEHeader.Magic -ne [System.Reflection.PortableExecutable.PEMagic]::PE32Plus) {
            throw "Expected an x64 executable: '$BinaryPath'."
        }
    } finally {
        $pe.Dispose()
    }
} finally {
    $stream.Dispose()
}
if ($binary.VersionInfo.FileVersion -cne $Version -or $binary.VersionInfo.ProductVersion -cne $Version) {
    throw "Executable file and product versions must both equal '$Version': '$BinaryPath'."
}

$kitsRoot = Get-ItemPropertyValue -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -Name KitsRoot10
$makeAppx = Get-ChildItem -LiteralPath (Join-Path $kitsRoot 'bin') -Directory |
    Where-Object { $_.Name -match '\A[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+\z' } |
    Sort-Object { [Version] $_.Name } -Descending |
    ForEach-Object { Join-Path $_.FullName 'x64\makeappx.exe' } |
    Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
    Select-Object -First 1
if (-not $makeAppx) {
    throw 'No x64 MakeAppx executable found. Install the Windows SDK packaging tools.'
}

$root = Split-Path -Parent $PSScriptRoot
$manifest = [xml]::new()
$manifest.Load((Join-Path $root 'packaging/msix/AppxManifest.xml'))
$manifest.Package.Identity.Version = $packageVersion
$executable = $manifest.Package.Applications.Application.Executable
$icons = Join-Path $root 'src-tauri/icons'
$assets = $manifest.SelectNodes("//*[local-name()='Logo']/text() | //@*[starts-with(., 'Assets\')]")

$outputRoot = [IO.Directory]::CreateDirectory($ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)).FullName
$outputPackage = Join-Path $outputRoot "OC2ModManager_${Version}_x64.msix"
if (Test-Path -LiteralPath $outputPackage) {
    throw "Refusing to overwrite '$outputPackage'."
}

$stage = (New-Item -ItemType Directory -Path (Join-Path $outputRoot ".oc2-msix-$([Guid]::NewGuid().ToString('N'))")).FullName
$stagedManifest = Join-Path $stage 'AppxManifest.xml'
$mapping = Join-Path $stage 'mapping.txt'
$stagedPackage = Join-Path $stage 'package.msix'
try {
    $manifest.Save($stagedManifest)
    $entries = @(
        '[Files]'
        '"{0}" "AppxManifest.xml"' -f $stagedManifest
        '"{0}" "{1}"' -f $binary.FullName, $executable
    )
    foreach ($asset in $assets) {
        $source = Join-Path $icons ([IO.Path]::GetFileName($asset.Value))
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "Missing icon for '$($asset.Value)': '$source'."
        }
        $entries += '"{0}" "{1}"' -f $source, $asset.Value
    }
    Set-Content -LiteralPath $mapping -Value $entries -Encoding Unicode
    $packLog = & $makeAppx pack /f $mapping /p $stagedPackage /no 2>&1
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $stagedPackage -PathType Leaf)) {
        throw "MakeAppx failed (exit $LASTEXITCODE):`n$($packLog -join "`n")"
    }
    Write-Verbose ($packLog -join "`n")
    [IO.File]::Move($stagedPackage, $outputPackage)
} finally {
    foreach ($temporary in @($stagedManifest, $mapping, $stagedPackage)) {
        if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) }
    }
    [IO.Directory]::Delete($stage)
}

$outputPackage
