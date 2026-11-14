# WinDivert64.sys для Windows 11 ARM64

Неподписанный драйвер WinDivert под ARM64 из набора автора zapret:
https://github.com/bol-van/zapret-win-bundle/blob/master/arm64/WinDivert64.sys

- SHA-256: `55CD5C827A28FC91448F2AD1AE629E3FC951F07726819AEB035969DFBF3712C9`
- Тип машины в PE-заголовке: `0xAA64` (ARM64)

На ARM64 приложение заменяет им обычный `engine\bin\WinDivert64.sys`. Сам `winws2.exe` (x64)
работает через встроенную эмуляцию Windows 11. Подписанного драйвера под ARM64 не существует,
поэтому Windows загрузит его только в режиме тестовой подписи (`bcdedit /set {current} testsigning on`,
Secure Boot должен быть выключен). Подробнее в разделе «Windows ARM64» [документации zapret2](https://github.com/bol-van/zapret2/blob/master/docs/manual.md).
