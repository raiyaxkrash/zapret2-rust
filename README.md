<div align="center">

# Zapret2 GUI

**Обход блокировок YouTube и Discord на Windows в один клик**

Удобное приложение для [zapret2](https://github.com/bol-van/zapret2): включение одной кнопкой, автоподбор стратегии, мини-окно в трее и автообновление.

[![Последний релиз](https://img.shields.io/github/v/release/raiyaxkrash/zapret2-rust?label=%D0%B2%D0%B5%D1%80%D1%81%D0%B8%D1%8F&color=db2777)](https://github.com/raiyaxkrash/zapret2-rust/releases/latest)
[![Сборка](https://img.shields.io/github/actions/workflow/status/raiyaxkrash/zapret2-rust/release.yml?label=%D1%81%D0%B1%D0%BE%D1%80%D0%BA%D0%B0)](https://github.com/raiyaxkrash/zapret2-rust/actions)
![Windows 10/11](https://img.shields.io/badge/Windows-10%20%7C%2011-7c3aed)
![Rust + Tauri 2](https://img.shields.io/badge/Rust-Tauri%202-f97316)

### [⬇ Скачать последнюю версию](https://github.com/raiyaxkrash/zapret2-rust/releases/latest)

</div>

---

## Возможности

- **Одна кнопка.** Включить и выключить обход можно в главном окне или в мини-окне в трее.
- **Готовые профили:** YouTube (включая QUIC), Discord, голосовые каналы Discord и «Мои сайты» со своим списком доменов.
- **Автоподбор стратегии.** Приложение по очереди пробует каждую стратегию и проверяет, открываются ли сайты. Для YouTube стратегия подбирается отдельно.
- **Мини-окно в трее:** обход, стратегия и сервисы без открытия главного окна.
- **Автообновление** приложения, стратегий, списков сайтов и самого zapret2.
- **Автозапуск с Windows** без запроса прав администратора при каждом входе.
- **Тёмная и светлая тема**, интерфейс почти не нагружает видеокарту.

## Установка

1. Скачайте `Zapret2.GUI_X.Y.Z_x64-setup.exe` со [страницы релизов](https://github.com/raiyaxkrash/zapret2-rust/releases/latest).
2. Запустите установщик и разрешите запуск от имени администратора: без этого не загрузится драйвер WinDivert.
3. Нажмите большую кнопку на главном экране.

Если что-то не открывается, откройте «Стратегии» и нажмите **«Подобрать автоматически»**.

> [!NOTE]
> Антивирусы иногда ошибочно считают драйвер WinDivert опасным. Это драйвер перехвата сетевых пакетов, а не вирус. Если обход не включается, проверьте карантин антивируса.

## Если YouTube не работает

1. «Стратегии» → «Подобрать автоматически» → «Применить».
2. Не помогло: выключите **QUIC / HTTP3** на главном экране и повторите подбор.
3. Всё ещё не работает: создайте [issue](https://github.com/raiyaxkrash/zapret2-rust/issues) и приложите результат подбора и содержимое вкладки «Журнал».

## Устройство репозитория

| Путь | Что там |
| --- | --- |
| [`gui/`](gui/) | Приложение Zapret2 GUI: Rust (Tauri 2) и интерфейс на HTML/CSS. Сборка, стратегии и выпуск версий описаны в [gui/README.md](gui/README.md) |
| [`gui/resources/presets.json`](gui/resources/presets.json) | Стратегии обхода. Изменения доходят до установленных копий без нового релиза |
| [`gui/resources/lists/`](gui/resources/lists/) | Встроенные списки доменов YouTube и Discord |
| [`.github/workflows/release.yml`](.github/workflows/release.yml) | Сборка и публикация подписанных релизов по тегу `vX.Y.Z` |
| `binaries/`, `lua/`, `files/`, `nfq2/` и др. | Исходный [zapret2](https://github.com/bol-van/zapret2) v1.0.7 от bol-van. Из него берутся `winws2.exe`, Lua-скрипты и фейковые пакеты для установщика |
| [`docs/`](docs/) | Документация zapret2: [manual.md](docs/manual.md) (синтаксис стратегий и параметров winws2) |

## Сборка из исходников

Нужны Rust (MSVC), Visual Studio Build Tools (C++) и Node.js.

```bash
cd gui
npm install
npm run build
```

Подробнее, в том числе о подписи обновлений, читайте в [gui/README.md](gui/README.md).

## Благодарности и лицензия

- [zapret2](https://github.com/bol-van/zapret2) от **bol-van**: движок обхода `winws2`, лицензия MIT ([docs/LICENSE.txt](docs/LICENSE.txt)).
- [WinDivert](https://reqrypt.org/windivert.html): драйвер перехвата пакетов.
- [Tauri](https://tauri.app): основа приложения.
