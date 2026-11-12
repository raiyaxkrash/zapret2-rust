<#
.SYNOPSIS
  Собирает консольный tg-ws-proxy (https://github.com/Flowseal/tg-ws-proxy, MIT) без трея
  и окон — его запускает Zapret2 GUI.

  Результат: gui/resources/tg-ws-proxy/ (tg-ws-proxy.exe, _internal, LICENSE.txt, version.txt).
  Сборка папкой, а не одним файлом: однофайловый exe при каждом запуске распаковывается
  во временную папку и оставляет её там, если процесс завершают принудительно.
  Если там уже лежит exe нужной версии, скрипт ничего не делает (-Force — пересобрать).
  Нужны git и Python 3.9+ (py или python в PATH).

.EXAMPLE
  .\gui\scripts\build-tg-proxy.ps1
  .\gui\scripts\build-tg-proxy.ps1 -Force
#>
param([switch]$Force)
$ErrorActionPreference = 'Stop'

# Версия и коммит закреплены: обновляя, меняйте оба значения.
$Tag = 'v1.11.4'
$Commit = '2085861957070eeea07990a522b29f42e5230210'
$Repo = 'https://github.com/Flowseal/tg-ws-proxy.git'

$gui = Split-Path -Parent $PSScriptRoot
$out = Join-Path $gui 'resources\tg-ws-proxy'
$exe = Join-Path $out 'tg-ws-proxy.exe'
$verFile = Join-Path $out 'version.txt'
$version = $Tag.TrimStart('v')

if (-not $Force -and (Test-Path $exe) -and (Test-Path $verFile) -and ((Get-Content $verFile -Raw).Trim() -eq $version)) {
    Write-Host "tg-ws-proxy $version уже собран: $exe"
    exit 0
}

function Invoke-Checked([string]$what, [scriptblock]$cmd) {
    & $cmd
    if ($LASTEXITCODE -ne 0) { throw "$what завершился с кодом $LASTEXITCODE" }
}

$python = if (Get-Command py -ErrorAction SilentlyContinue) { @('py', '-3') }
          elseif (Get-Command python -ErrorAction SilentlyContinue) { @('python') }
          else { throw 'Не найден Python 3 (py или python в PATH)' }

$work = Join-Path ([IO.Path]::GetTempPath()) ("tg-ws-proxy-build-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $work | Out-Null
try {
    Write-Host "Загрузка tg-ws-proxy $Tag…"
    $src = Join-Path $work 'src'
    Invoke-Checked 'git clone' { git -c advice.detachedHead=false clone -q --depth 1 --branch $Tag $Repo $src }
    $head = (git -C $src rev-parse HEAD).Trim()
    if ($head -ne $Commit) { throw "Тег $Tag указывает на $head, ожидался $Commit. Проверьте репозиторий." }

    Write-Host 'Подготовка окружения Python…'
    $venv = Join-Path $work 'venv'
    $pyArgs = @($python | Select-Object -Skip 1) + @('-m', 'venv', $venv)
    Invoke-Checked 'venv' { & $python[0] @pyArgs }
    $py = Join-Path $venv 'Scripts\python.exe'
    Invoke-Checked 'pip' { & $py -m pip install -q --disable-pip-version-check `
        'cryptography==46.0.5' 'httpx[http2]==0.28.1' 'certifi' 'pyinstaller==6.11.1' }

    # Точка входа: только сам прокси, без трея, окон настроек и самообновления.
    $entry = Join-Path $src 'tg_ws_proxy_cli.py'
    Set-Content -Encoding UTF8 $entry @'
from proxy.tg_ws_proxy import main

if __name__ == "__main__":
    main()
'@

    Write-Host 'Сборка exe…'
    Push-Location $src
    try {
        Invoke-Checked 'pyinstaller' { & $py -m PyInstaller --noconfirm --clean --onedir --console `
            --name tg-ws-proxy --paths $src `
            --collect-submodules proxy --collect-data certifi `
            --hidden-import h2 `
            --hidden-import cryptography.hazmat.primitives.ciphers `
            --hidden-import cryptography.hazmat.primitives.ciphers.algorithms `
            --hidden-import cryptography.hazmat.primitives.ciphers.modes `
            --exclude-module tkinter --exclude-module customtkinter --exclude-module PIL `
            --exclude-module pystray --exclude-module psutil --exclude-module pyperclip `
            --distpath (Join-Path $work 'dist') --workpath (Join-Path $work 'build') --specpath $work `
            $entry }
    } finally { Pop-Location }

    if (Test-Path $out) { Remove-Item -Recurse -Force $out }
    Copy-Item -Recurse (Join-Path $work 'dist\tg-ws-proxy') $out
    Copy-Item (Join-Path $src 'LICENSE') (Join-Path $out 'LICENSE.txt') -Force
    [IO.File]::WriteAllText($verFile, $version, (New-Object System.Text.UTF8Encoding $false))
    $size = [math]::Round(((Get-ChildItem -Recurse -File $out | Measure-Object Length -Sum).Sum) / 1MB, 1)
    Write-Host "Готово: $exe ($size МБ)"
} finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
