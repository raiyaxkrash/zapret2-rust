# Zapret2 GUI

Графическая оболочка для [zapret2](https://github.com/bol-van/zapret2) под Windows 10/11 (x64).
Внутри работает `winws2.exe` с драйвером WinDivert из этого же репозитория. Приложение написано на Rust (Tauri 2), интерфейс сделан на HTML/CSS.

## Возможности

- Собственный яркий дизайн: градиенты, «стеклянные» карточки, анимации, тёмная и светлая тема.
- Мини-окно в трее: включение обхода, выбор стратегии и сервисов, не открывая главное окно. Открывается щелчком по значку, двойной щелчок открывает главное окно.
- Отдельная стратегия для YouTube: автоподбор оценивает YouTube и остальные сервисы раздельно.
- При выключенном QUIC он блокируется для YouTube и Discord, чтобы браузер сразу переходил на TCP.
- Включение и выключение обхода одной кнопкой.
- Готовые профили: **YouTube** (включая QUIC), **Discord**, **голосовые каналы Discord**, **«Мои сайты»**.
- «Мои сайты» в трёх режимах: только из списка, список с автодобавлением заблокированных доменов, все сайты, кроме исключений.
- 7 готовых стратегий плюс свои параметры `--lua-desync`.
- **Автоподбор стратегии**: приложение по очереди запускает каждую стратегию и проверяет, открываются ли YouTube, Discord и ваши сайты (загружает не меньше 64 КБ, чтобы поймать обрыв после 16 КБ).
- Редактор списков сайтов и исключений.
- Журнал работы winws2 и показ итоговой командной строки.
- Автозапуск вместе с Windows через Планировщик заданий (с правами администратора, без запроса UAC).
- **Автообновление zapret2**: приложение проверяет релизы `bol-van/zapret2`, сверяет SHA-256 и заменяет `winws2`, Lua-скрипты и фейки.
- **Автообновление приложения** через подписанные релизы (Tauri updater).

## Установка

Запустите `Zapret2 GUI_1.0.0_x64-setup.exe`. Приложению нужны права администратора: без них драйвер WinDivert не загрузится.

Антивирусы иногда ошибочно считают WinDivert опасным. Если обход не включается, проверьте карантин антивируса.

Рабочие файлы хранятся в `%LOCALAPPDATA%\Zapret2GUI`:

| Путь | Что там |
| --- | --- |
| `engine\` | winws2, Lua-скрипты, фейки, фильтры WinDivert (обновляются автоматически) |
| `lists\user.txt` | Мои сайты |
| `lists\exclude.txt` | Исключения |
| `lists\youtube.txt`, `lists\discord.txt` | Встроенные списки (обновляются вместе с приложением) |
| `settings.json` | Настройки |

Автосписок лежит в `%USERPROFILE%\AppData\LocalLow\Zapret2GUI\autohostlist.txt`, потому что после старта winws2 понижает себе права и может писать только туда.

## Сборка

Нужны: Rust (stable, MSVC), Visual Studio Build Tools с компонентом «Разработка классических приложений на C++», Node.js.

```bash
cd gui
npm install
npm run build
```

Установщик появится в `gui/src-tauri/target/release/bundle/nsis/`.

Сборка с файлами автообновления требует ключ подписи. Он уже создан в `gui/.keys/zapret2gui.key` (без пароля). **Сохраните его в надёжном месте**: без этого ключа уже установленные копии не примут обновления.

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content .keys\zapret2gui.key -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
npm run build
```

## Как включить автообновление приложения

Приложение ищет обновления по адресу `https://github.com/raiyaxkrash/zapret2-rust/releases/latest/download/latest.json` (константа `DEFAULT_APP_UPDATE_URL` в `src-tauri/src/settings.rs`; можно переопределить в Настройках → Обновления).

1. В настройках репозитория (Settings → Secrets and variables → Actions) добавьте секрет `TAURI_SIGNING_PRIVATE_KEY` с содержимым `gui/.keys/zapret2gui.key`. Сам каталог `.keys` в репозиторий не попадает, см. `.gitignore`.
2. Поднимите версию в `gui/src-tauri/tauri.conf.json`, `gui/src-tauri/Cargo.toml` и `gui/package.json`, затем отправьте тег `vX.Y.Z`. Workflow `.github/workflows/release.yml` соберёт установщик и опубликует релиз вместе с `latest.json`.

## Стратегии

Стратегии описаны в `gui/resources/presets.json`, их можно менять без пересборки Rust-кода.

**Обновление без нового релиза.** Установленные копии скачивают `gui/resources/presets.json` и `gui/resources/lists/{youtube,discord}.txt` из ветки `main` этого репозитория. Новые файлы применяются, только если поле `"version"` в `presets.json` больше текущего. Порядок такой: правите стратегии или списки, увеличиваете `"version"` на 1, отправляете коммит в `main`. Приложения подхватят изменения при запуске и затем каждые 6 часов. Также можно нажать «Обновить» в Настройках → Обновления. Каждая стратегия задаёт аргументы для TLS (`tls`), HTTP (`http`) и, при необходимости, для QUIC (`quic`). Синтаксис описан в `docs/manual.md`, разделы «Полный список опций» и «Библиотека программ атаки на DPI».

Из выбранных сервисов и стратегии приложение собирает профили winws2. Например, для YouTube:

```
--wf-tcp-out=80,443 --wf-udp-out=443
--lua-init=@engine/lua/zapret-lib.lua --lua-init=@engine/lua/zapret-antidpi.lua
--blob=tls_google:@engine/fake/tls_clienthello_www_google_com.bin ...
--filter-tcp=80,443 --filter-l7=tls,http --hostlist=lists/youtube.txt <стратегия> --new
--filter-udp=443 --filter-l7=quic --hostlist=lists/youtube.txt --payload=quic_initial --lua-desync=fake:blob=quic_google:repeats=6
```

Полную командную строку для текущих настроек можно посмотреть и скопировать на вкладке «Стратегии».
