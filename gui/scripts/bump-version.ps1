<#
.SYNOPSIS
  Меняет версию Zapret2 GUI сразу во всех файлах и (по желанию) создаёт коммит и тег.

.EXAMPLE
  .\gui\scripts\bump-version.ps1 1.4.0
  .\gui\scripts\bump-version.ps1 1.4.0 -Tag      # плюс коммит и тег v1.4.0 (без отправки)
#>
param(
    [Parameter(Mandatory = $true)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version,
    [switch]$Tag
)
$ErrorActionPreference = 'Stop'
$gui = Split-Path -Parent $PSScriptRoot
$utf8 = New-Object System.Text.UTF8Encoding $false

function Set-Version([string]$file, [string]$pattern, [string]$replacement) {
    $path = Join-Path $gui $file
    $text = [IO.File]::ReadAllText($path)
    $new = [regex]::Replace($text, $pattern, $replacement, 'Multiline')
    if (-not [regex]::IsMatch($text, $pattern, 'Multiline')) { throw "Не удалось найти версию в $file" }
    [IO.File]::WriteAllText($path, $new, $utf8)
    Write-Host "  $file"
}

Write-Host "Версия -> $Version"
Set-Version 'src-tauri/tauri.conf.json' '^(\s*"version":\s*)"[^"]+"' "`$1`"$Version`""
Set-Version 'src-tauri/Cargo.toml' '^version = "[^"]+"' "version = `"$Version`""
Set-Version 'package.json' '^(\s*"version":\s*)"[^"]+"' "`$1`"$Version`""

Push-Location $gui
try {
    cmd /c "npm install --package-lock-only --no-fund --no-audit >nul 2>&1"
    cmd /c "cd src-tauri && cargo update -p zapret2-gui --offline >nul 2>&1"
    if ($Tag) {
        git add -A
        git commit -m "Zapret2 GUI $Version"
        git tag -a "v$Version" -m "Zapret2 GUI $Version"
        Write-Host "Готово. Отправка: git push origin main v$Version"
    }
} finally {
    Pop-Location
}
