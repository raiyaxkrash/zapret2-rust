<div align="center">

# Zapret2 GUI

**YouTube, Discord, Telegram и другие сайты на Windows — в один клик**

Простое приложение для [zapret2](https://github.com/bol-van/zapret2): одна кнопка, мастер настройки, автоподбор стратегии, встроенный прокси для Telegram Desktop, готовые наборы сайтов и автообновление.

[English](README.en.md)

[![Последний релиз](https://img.shields.io/github/v/release/raiyaxkrash/zapret2-rust?label=%D0%B2%D0%B5%D1%80%D1%81%D0%B8%D1%8F&color=0a0a0a)](https://github.com/raiyaxkrash/zapret2-rust/releases/latest)
[![Сборка](https://img.shields.io/github/actions/workflow/status/raiyaxkrash/zapret2-rust/release.yml?label=%D1%81%D0%B1%D0%BE%D1%80%D0%BA%D0%B0)](https://github.com/raiyaxkrash/zapret2-rust/actions)
![Windows 10/11](https://img.shields.io/badge/Windows-10%20%7C%2011%20%7C%20ARM64-555555)
![Rust + Tauri 2](https://img.shields.io/badge/Rust-Tauri%202-555555)

### [⬇ Скачать последнюю версию](https://github.com/raiyaxkrash/zapret2-rust/releases/latest)

<img src="gui/screenshots/main-dark.png" alt="Главный экран Zapret2 GUI" width="820">

</div>

---

## Возможности

- **Одна кнопка.** Нажмите на сферу на главной или включите обход из мини-окна в трее.
- **Мастер первого запуска.** Отметьте, что не открывается, и приложение само подберёт настройки примерно за минуту.
- **Готовые профили:** YouTube (включая QUIC), Discord, голосовые каналы Discord и «Мои сайты» со своим списком доменов.
- **Прокси для Telegram Desktop.** Встроенный локальный прокси: приложение Telegram подключается через WebSocket к серверам Telegram, настраивать обход не нужно. Подключение одной кнопкой.
- **Наборы сайтов одной кнопкой:** Telegram, Instagram и Facebook, X (Twitter), торрент-трекеры, Roblox, Signal, LinkedIn.
- **Зарубежные хостинги и CDN.** Для сайтов и игр на Cloudflare, Hetzner, DigitalOcean, OVH, AWS и других хостингах, которые обрываются или грузятся бесконечно.
- **Автоподбор стратегии.** Приложение пробует каждую стратегию и проверяет, открываются ли сайты. Для YouTube стратегия подбирается отдельно.
- **Подсказка для вашего провайдера.** Приложение определяет провайдера и, если для него известна рабочая стратегия, предлагает её. Результатом подбора можно поделиться, чтобы подсказка появилась у других.
- **Проверка любого сайта:** вставьте ссылку, и приложение покажет, открывается ли сайт без обхода и с какими стратегиями.
- **Видно, что работает:** доступность сервисов («YouTube 320 мс, Discord 180 мс») и график трафика за 10 минут прямо на главной.
- **Надёжно:** если winws2 упадёт, приложение перезапустит его само, а при выходе он не остаётся висеть.
- **Автообновление** приложения, стратегий, списков сайтов и самого zapret2.
- **Автозапуск с Windows** без запроса прав администратора при каждом входе.
- **Строгий чёрно-белый дизайн**, тёмная и светлая тема, интерфейс почти не нагружает компьютер.
- **Windows 11 на ARM64** (Snapdragon): приложение само ставит нужный драйвер.

<table>
  <tr>
    <td><img src="gui/screenshots/wizard.png" alt="Мастер первого запуска"></td>
    <td><img src="gui/screenshots/strategies.png" alt="Подбор стратегии"></td>
  </tr>
  <tr>
    <td align="center">Мастер первого запуска</td>
    <td align="center">Автоподбор: YouTube и остальные сервисы отдельно</td>
  </tr>
  <tr>
    <td><img src="gui/screenshots/telegram.png" alt="Прокси для Telegram"></td>
    <td><img src="gui/screenshots/packs.png" alt="Наборы сайтов"></td>
  </tr>
  <tr>
    <td align="center">Прокси для Telegram Desktop</td>
    <td align="center">Наборы сайтов одной кнопкой</td>
  </tr>
  <tr>
    <td><img src="gui/screenshots/main-light.png" alt="Светлая тема"></td>
    <td align="center"><img src="gui/screenshots/tray.png" alt="Мини-окно в трее" width="300"></td>
  </tr>
  <tr>
    <td align="center">Светлая тема</td>
    <td align="center">Мини-окно в трее</td>
  </tr>
</table>

## Установка

1. Скачайте `Zapret2.GUI_X.Y.Z_x64-setup.exe` со [страницы релизов](https://github.com/raiyaxkrash/zapret2-rust/releases/latest).
2. Запустите установщик и разрешите запуск от имени администратора: без этого не загрузится драйвер WinDivert.
3. При первом запуске откроется мастер: отметьте, что у вас не открывается, и нажмите «Настроить». Дальше обход включается и выключается нажатием на сферу на главной.

> [!NOTE]
> Антивирусы иногда ошибочно считают драйвер WinDivert опасным. Это драйвер перехвата сетевых пакетов, а не вирус. Если обход не включается, проверьте карантин антивируса.
>
> Установщик собирается автоматически на GitHub Actions из открытого исходного кода этого репозитория: журнал каждой сборки можно посмотреть на вкладке [Actions](https://github.com/raiyaxkrash/zapret2-rust/actions).

## Вопросы и ответы

<details>
<summary><b>YouTube не работает, а Discord работает</b></summary>

Серверам Google часто нужна другая стратегия. Откройте «Стратегии» → «Подобрать автоматически» → «Применить»: подбор выставит отдельную стратегию для YouTube. Если не помогло, выключите **QUIC / HTTP3** на странице «Стратегии» и повторите подбор.
</details>

<details>
<summary><b>Зачем нужны права администратора?</b></summary>

zapret2 перехватывает сетевые пакеты через драйвер WinDivert, а загружать драйверы в Windows может только администратор. Автозапуск сделан через Планировщик заданий, поэтому запрос UAC при каждом входе не появляется.
</details>

<details>
<summary><b>Как подключить приложение Telegram?</b></summary>

Включите «Прокси для Telegram» на главной, откройте раздел «Telegram» и нажмите «Подключить в Telegram»: Telegram Desktop сам предложит добавить прокси, это нужно сделать один раз. Прокси работает отдельно от обхода, его можно включить, даже если обход выключен. Набор сайтов «Telegram» нужен только для сайта, веб-версии и ссылок t.me.
</details>

<details>
<summary><b>Некоторые сайты или игры грузятся бесконечно</b></summary>

Часто это сайты на зарубежных хостингах (Cloudflare, Hetzner, AWS и другие): соединение с ними обрывается после первых килобайт. Включите набор «Зарубежные хостинги и CDN» на странице «Списки сайтов». Если сайт всё равно не открывается, проверьте его через «Стратегии» → «Проверить сайт».
</details>

<details>
<summary><b>Чем это отличается от GoodbyeDPI и сборок zapret?</b></summary>

Внутри работает тот же zapret2 от bol-van, но настраивать ничего вручную не нужно: стратегия подбирается автоматически, списки и стратегии обновляются сами, есть встроенный прокси для Telegram. Одновременно с другими программами обхода приложение работать не может, они используют тот же драйвер.
</details>

<details>
<summary><b>Конфликт с другими программами обхода</b></summary>

GoodbyeDPI, zapret-discord-youtube и другие сборки winws используют тот же драйвер. Если они запущены, на главном экране появится предупреждение с кнопкой «Завершить».
</details>

<details>
<summary><b>Windows 11 на ARM64 (Snapdragon)</b></summary>

Подписанного драйвера WinDivert для ARM64 не существует. Приложение само подкладывает неподписанный ARM64-драйвер из набора автора zapret, но Windows загрузит его только в **режиме тестовой подписи**: выключите Secure Boot в настройках UEFI, нажмите «Включить тестовый режим» на главном экране и перезагрузите компьютер. В углу рабочего стола появится надпись «Тестовый режим». Отключить его можно командой `bcdedit /set {current} testsigning off`.
</details>

<details>
<summary><b>Где хранятся настройки и списки?</b></summary>

В `%LOCALAPPDATA%\Zapret2GUI`. Открыть папку можно в Настройках → «Папка с данными».
</details>

<details>
<summary><b>Как удалить?</b></summary>

Через «Параметры Windows» → «Приложения». Задание автозапуска и запущенный winws2 удаляются вместе с приложением.
</details>

Не нашли ответа? [Создайте issue](https://github.com/raiyaxkrash/zapret2-rust/issues/new/choose) и приложите результат подбора и журнал.

## Устройство репозитория

| Путь | Что там |
| --- | --- |
| [`gui/`](gui/) | Приложение Zapret2 GUI: Rust (Tauri 2) и интерфейс на HTML/CSS. Сборка, стратегии и выпуск версий описаны в [gui/README.md](gui/README.md) |
| [`gui/resources/presets.json`](gui/resources/presets.json) | Стратегии и наборы сайтов. Изменения доходят до установленных копий без нового релиза |
| [`gui/resources/lists/`](gui/resources/lists/) | Встроенные списки доменов (YouTube, Discord и наборы) |
| [`.github/workflows/release.yml`](.github/workflows/release.yml) | Тесты, сборка и публикация подписанных релизов по тегу `vX.Y.Z` |
| `binaries/`, `lua/`, `files/`, `nfq2/` и др. | Исходный [zapret2](https://github.com/bol-van/zapret2) v1.0.7 от bol-van. Из него берутся `winws2.exe`, Lua-скрипты и фейковые пакеты для установщика |
| [`docs/`](docs/) | Документация zapret2: [manual.md](docs/manual.md) (синтаксис стратегий и параметров winws2) |

## Сборка из исходников

Нужны Rust (MSVC), Visual Studio Build Tools (C++) и Node.js.

```bash
cd gui
npm install
npm run build
```

Подробнее, в том числе о тестах и подписи обновлений, читайте в [gui/README.md](gui/README.md).

## Благодарности и лицензия

- [zapret2](https://github.com/bol-van/zapret2) и [zapret-win-bundle](https://github.com/bol-van/zapret-win-bundle) от **bol-van**: движок обхода `winws2` и ARM64-драйвер, лицензия MIT ([docs/LICENSE.txt](docs/LICENSE.txt)).
- [WinDivert](https://reqrypt.org/windivert.html): драйвер перехвата пакетов.
- [Tauri](https://tauri.app): основа приложения.
