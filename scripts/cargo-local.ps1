# Use the optional workspace-local Rust installation without changing global PATH.
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$cargoPath = Join-Path $projectRoot '.tools\cargo\bin\cargo.exe'
if (-not (Test-Path -LiteralPath $cargoPath)) {
    throw 'No local toolchain. Install Rust from https://rustup.rs/ and use cargo directly.'
}
$env:CARGO_HOME = Join-Path $projectRoot '.tools\cargo'
$env:RUSTUP_HOME = Join-Path $projectRoot '.tools\rustup'
$env:PATH = (Join-Path $projectRoot '.tools\cargo\bin') + ';' + $env:PATH
# Optional local SDK fallback for machines with incomplete VS SDK installations.
$sdkRoot = Join-Path $projectRoot '.tools\windows-sdk\c'
if (Test-Path -LiteralPath $sdkRoot) {
    $compiler = Get-ChildItem 'C:\Program Files (x86)\Microsoft Visual Studio\*\BuildTools\VC\Tools\MSVC\*\bin\Hostx64\x64\link.exe' -ErrorAction SilentlyContinue |
        Sort-Object FullName -Descending | Select-Object -First 1
    if ($compiler) {
        $compilerBin = Split-Path -Parent $compiler.FullName
        $msvcRoot = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $compilerBin))
        $env:PATH = $compilerBin + ';' + $env:PATH
        $env:LIB = (Join-Path $msvcRoot 'lib\x64') + ';' + (Join-Path $sdkRoot 'um\x64') + ';' + (Join-Path $sdkRoot 'ucrt\x64')
    }
}
& $cargoPath @args
exit $LASTEXITCODE
