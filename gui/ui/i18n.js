"use strict";
/* Перевод интерфейса на английский.
 * Интерфейс написан по-русски. В английском режиме переводчик проходит по тексту и атрибутам страницы
 * и заменяет строки по словарю, а MutationObserver переводит всё, что появляется потом.
 * Записи с {} — шаблоны: подставленные значения переводятся отдельно (например, «3 сайта» внутри фразы).
 * Журнал, командная строка и поля ввода не переводятся. */

const I18N = (() => {
  const KEY = "z2.lang";
  const RU_SYSTEM = /^(ru|uk|be|kk)\b/i;

  /** Язык из URL (?lang=en), из прошлого запуска или из языка системы. */
  function initial() {
    const q = new URLSearchParams(location.search).get("lang");
    if (q === "ru" || q === "en") return q;
    try { const v = localStorage.getItem(KEY); if (v === "ru" || v === "en") return v; } catch {}
    return RU_SYSTEM.test(navigator.language || "ru") ? "ru" : "en";
  }
  const lang = initial();
  const en = lang === "en";

  /* --------------------------------------------------------------- словарь */
  const D = [
    // окно и навигация
    ["Установить обновление", "Install update"],
    ["Обновление", "Update"],
    ["Свернуть", "Minimize"],
    ["Развернуть", "Maximize"],
    ["Восстановить", "Restore"],
    ["Закрыть", "Close"],
    ["Главная", "Home"],
    ["Стратегии", "Strategies"],
    ["Списки сайтов", "Site lists"],
    ["Диагностика", "Diagnostics"],
    ["Журнал", "Log"],
    ["Настройки", "Settings"],

    // главная
    ["Выключен", "Off"],
    ["Включить или выключить обход", "Turn the bypass on or off"],
    ["Обход выключен", "Bypass is off"],
    ["Обход включён", "Bypass is on"],
    ["Нажмите на сферу, чтобы включить", "Click the sphere to turn it on"],
    ["Нажмите, чтобы включить", "Click to turn on"],
    ["Проверить доступность", "Check availability"],
    ["Что обходить", "What to unblock"],
    ["Видео и превью", "Videos and thumbnails"],
    ["Сайт и приложение", "Website and app"],
    ["Голос Discord", "Discord voice"],
    ["Звонки и каналы", "Calls and channels"],
    ["Мои сайты", "My sites"],
    ["Сайты из вашего списка", "Sites from your list"],
    ["Нейросети и другие сервисы", "AI and other services"],
    ["Прокси для Telegram", "Telegram proxy"],
    ["Для приложения Telegram", "For the Telegram app"],
    ["Наборы сайтов", "Site packs"],
    ["Подключить Telegram", "Connect Telegram"],
    ["Трафик", "Traffic"],
    ["10 мин", "10 min"],
    ["Не открывается сайт? Вставьте ссылку или домен и нажмите Enter", "Site won't open? Paste a link or domain and press Enter"],
    ["Добавить", "Add"],
    ["Подбор стратегии…", "Finding a strategy…"],
    ["Подождите…", "Please wait…"],
    ["Идёт подбор стратегии", "Finding a strategy"],
    ["Перезапуск…", "Restarting…"],
    ["Запуск…", "Starting…"],
    ["Выключение…", "Turning off…"],
    ["Дождитесь окончания проверки на странице «Стратегии»", "Wait for the check on the “Strategies” page to finish"],
    ["Заблокированные сервисы должны открываться", "Blocked services should open now"],
    ["Запущены другие программы обхода ({}). Они конфликтуют с Zapret2.", "Other bypass programs are running ({}). They conflict with Zapret2."],
    ["Запущены другие программы обхода. Откройте главное окно, чтобы их завершить.", "Other bypass programs are running. Open the main window to close them."],
    ["Завершить", "Close them"],
    ["{}. Проверьте подключение к интернету.", "{}. Check your internet connection."],
    ["Повторить", "Retry"],
    ["Компьютер на ARM64.", "ARM64 computer."],
    ["Для ARM64 нет подписанного драйвера WinDivert, поэтому Windows загрузит его только в режиме тестовой подписи. Сначала выключите Secure Boot в настройках UEFI/BIOS, затем включите тестовый режим здесь.",
      "There is no signed WinDivert driver for ARM64, so Windows loads it only in test signing mode. First turn off Secure Boot in the UEFI/BIOS settings, then turn on test mode here."],
    ["Для ARM64 нет подписанного драйвера WinDivert, поэтому Windows загрузит его только в режиме тестовой подписи. Включите тестовый режим и перезагрузите компьютер.",
      "There is no signed WinDivert driver for ARM64, so Windows loads it only in test signing mode. Turn on test mode and restart the computer."],
    ["Включить тестовый режим", "Turn on test mode"],
    ["Загрузка zapret2…", "Downloading zapret2…"],
    ["Вкл.", "On"],
    ["Откл.", "Off"],
    ["Все сайты, кроме исключений", "All sites except exclusions"],
    ["{} + {} найдено автоматически", "{} + {} found automatically"],
    ["{} в списке", "{} in the list"],
    ["Список пуст — добавьте сайты", "The list is empty: add sites"],
    ["Настройки применены, обход перезапущен", "Settings applied, bypass restarted"],
    ["Завершено процессов: {}", "Processes closed: {}"],
    ["проверка…", "checking…"],
    ["не открывается", "doesn't open"],
    ["{}/{} · {} мс", "{}/{} · {} ms"],
    ["Проверено в {} · обход выключен", "Checked at {} · bypass is off"],
    ["Проверено в {}", "Checked at {}"],
    ["· включено {}", "· {} on"],
    ["Работает {}", "Running for {}"],
    ["Работает · 127.0.0.1:{}", "Running · 127.0.0.1:{}"],
    ["Ошибка, подробнее в разделе Telegram", "Error, see the Telegram page"],
    ["Ошибка, подробнее в разделе WARP", "Error, see the WARP page"],
    ["Не входит в эту сборку приложения", "Not included in this build"],
    ["Подбор стратегии", "Finding a strategy"],

    // единицы и числа
    ["сайт", "site"], ["сайтов", "sites"],
    ["домен", "domain"], ["доменов", "domains"],
    ["подсеть", "subnet"], ["подсетей", "subnets"],
    ["{} ч {} мин", "{} h {} min"],
    ["{} мин {} с", "{} min {} s"],
    ["{} с", "{} s"],
    ["{} мс", "{} ms"],
    ["{} МБ/с", "{} MB/s"],
    ["{} КБ/с", "{} KB/s"],
    ["{} Б/с", "{} B/s"],
    ["{} ТБ", "{} TB"],
    ["{} ГБ", "{} GB"],
    ["{} МБ", "{} MB"],
    ["{} КБ", "{} KB"],
    ["{} Б", "{} B"],

    // WARP
    ["Бесплатный туннель Cloudflare для сервисов, которые сами закрыли доступ из вашего региона: нейросети, музыка, загрузки программ. Через WARP идут только выбранные сервисы, остальное — напрямую.",
      "A free Cloudflare tunnel for services that block access from your region themselves: AI, music, software downloads. Only the selected services go through WARP, everything else goes direct."],
    ["Включить WARP", "Turn on WARP"],
    ["Проверить", "Check"],
    ["Подключить снова", "Reconnect"],
    ["Что пускать через WARP", "What goes through WARP"],
    ["Свои сайты", "Custom sites"],
    ["По одному домену в строке, поддомены учитываются автоматически.", "One domain per line, subdomains are included automatically."],
    ["Сохранить", "Save"],
    ["Параметры", "Options"],
    ["Маскировка", "Obfuscation"],
    ["Перед подключением отправлять случайные пакеты, чтобы провайдер не узнал WARP. Выключайте, только если без неё работает лучше",
      "Send random packets before connecting so the ISP doesn't recognize WARP. Turn it off only if things work better without it"],
    ["Порт локального прокси", "Local proxy port"],
    ["Для программ", "For apps"],
    ["Пускать через WARP и программы, которые не используют системный прокси: Spotify, ChatGPT и Claude для компьютера. Работает через драйвер WinDivert из zapret2",
      "Also route apps that ignore the system proxy through WARP: Spotify, ChatGPT and Claude desktop apps. Uses the WinDivert driver from zapret2"],
    ["{} · браузеры и программы", "{} · browsers and apps"],
    ["WARP для программ включён", "WARP for apps is on"],
    ["WARP для программ: zapret2 ещё не загружен", "WARP for apps: zapret2 isn't downloaded yet"],
    ["WARP для программ: {}", "WARP for apps: {}"],
    ["Не удалось загрузить {}", "Couldn't load {}"],
    ["WinDivert: ошибка {}", "WinDivert: error {}"],
    ["По умолчанию 40080. Браузеры получают его автоматически через системные настройки прокси", "Default is 40080. Browsers pick it up automatically from the system proxy settings"],
    ["Получено {}, отправлено {}", "Received {}, sent {}"],
    ["Подключение…", "Connecting…"],
    ["Не подключён", "Not connected"],
    ["Подключён через {} · {}", "Connected via {} · {}"],
    ["Переподключение…", "Reconnecting…"],
    ["Подключён", "Connected"],
    ["Сайты для WARP сохранены", "WARP sites saved"],
    ["Работает: ваш адрес через WARP {}, страна {}", "Working: your address via WARP is {}, country {}"],
    ["Ответ получен, но WARP не активен ({})", "Got a response, but WARP isn't active ({})"],

    // диагностика
    ["Проверка того, что именно мешает: подмена DNS, блокировка по IP, обрыв соединения или ограничение самого сервиса. Займёт около 20 секунд.",
      "Finds out what exactly is in the way: DNS spoofing, IP blocking, dropped connections or restrictions by the service itself. Takes about 20 seconds."],
    ["Скопировать отчёт", "Copy report"],
    ["Проверяем…", "Checking…"],
    ["Идёт проверка…", "Checking…"],
    ["Проверить снова", "Check again"],
    ["Отчёт скопирован", "Report copied"],

    // Telegram
    ["Встроенный прокси для приложения Telegram Desktop. Трафик идёт к серверам Telegram через WebSocket, обход для этого не нужен.",
      "Built-in proxy for the Telegram Desktop app. Traffic goes to Telegram servers over WebSocket, so no bypass is needed."],
    ["Прокси для Telegram Desktop", "Telegram Desktop proxy"],
    ["Приложение Telegram подключится через WebSocket, без настройки обхода", "The Telegram app connects over WebSocket, no bypass setup needed"],
    ["Нажмите «Подключить» и подтвердите в Telegram. Это нужно сделать один раз.", "Click “Connect” and confirm in Telegram. You only need to do this once."],
    ["Скопировать ссылку", "Copy link"],
    ["Подключить в Telegram", "Connect in Telegram"],
    ["Запустить снова", "Start again"],
    ["Порт", "Port"],
    ["По умолчанию 1443. После смены порта заново подключите прокси в Telegram", "Default is 1443. After changing the port, connect the proxy in Telegram again"],
    ["Работает на 127.0.0.1:{}", "Running on 127.0.0.1:{}"],
    ["Не запущен", "Not running"],
    ["Подтвердите подключение к прокси в окне Telegram", "Confirm the proxy connection in the Telegram window"],
    ["Ссылка скопирована. Отправьте её себе в Telegram и откройте", "Link copied. Send it to yourself in Telegram and open it"],
    ["Порт должен быть числом от 1024 до 65535", "The port must be a number from 1024 to 65535"],
    ["Порт сохранён: {}", "Port saved: {}"],

    // стратегии
    ["Способ, которым обманывается система блокировки. У разных провайдеров работают разные стратегии.",
      "The method used to trick the blocking system. Different strategies work for different ISPs."],
    ["Остановить", "Stop"],
    ["Подобрать автоматически", "Find automatically"],
    ["Проверить сайт: вставьте ссылку, и приложение проверит его с каждой стратегией", "Check a site: paste a link and the app will test it with every strategy"],
    ["Стратегия для YouTube", "YouTube strategy"],
    ["Серверам Google часто нужна другая стратегия, чем Discord и остальным сайтам", "Google servers often need a different strategy than Discord and other sites"],
    ["Вкл. — обход и для QUIC. Откл. — QUIC к YouTube и Discord блокируется, и браузер сразу переходит на TCP. Если видео не грузится, попробуйте выключить",
      "On: bypass QUIC too. Off: QUIC to YouTube and Discord is blocked and the browser switches to TCP right away. If videos won't load, try turning it off"],
    ["Основная стратегия", "Main strategy"],
    ["Командная строка winws2", "winws2 command line"],
    ["Итоговые параметры для текущих настроек", "Resulting arguments for the current settings"],
    ["Копировать", "Copy"],
    ["Как основная", "Same as main"],
    ["ошибка", "error"],
    ["Остальные", "Others"],
    ["Остальные {}", "Others {}"],
    ["лучшая", "best"],
    ["лучшая для YouTube", "best for YouTube"],
    ["выбрана для YouTube", "selected for YouTube"],
    ["QUIC (пусто — как в остальных стратегиях)", "QUIC (empty: same as other strategies)"],
    ["Сохранить параметры", "Save options"],
    ["Показать параметры", "Show arguments"],
    ["Скрыть параметры", "Hide arguments"],
    ["Параметры сохранены", "Options saved"],
    ["Скопировано", "Copied"],
    ["Без обхода", "No bypass"],
    ["Проверка сайта {}", "Checking {}"],
    ["Сайт проверяется без обхода и с каждой стратегией по очереди.", "The site is checked without bypass and then with each strategy in turn."],
    ["Каждая стратегия запускается по очереди, и проверяется, открываются ли сайты. Это займёт около минуты.",
      "Each strategy is started in turn to see whether sites open. It takes about a minute."],
    ["Проверка {} остановлена", "Check of {} stopped"],
    ["Проверка {} завершена", "Check of {} finished"],
    ["{} открывается и без обхода: блокировки нет или она частичная.", "{} opens without bypass too: there's no blocking or it's partial."],
    ["{} открывается со стратегиями: {}.", "{} opens with these strategies: {}."],
    ["{} не открылся ни с одной стратегией. Возможно, сайт заблокирован по IP или сейчас не работает.",
      "{} didn't open with any strategy. The site may be blocked by IP or be down right now."],
    ["Текущая стратегия для него не подходит, быстрее всех «{}».", "The current strategy doesn't work for it; the fastest is “{}”."],
    ["Текущая стратегия для него не подходит.", "The current strategy doesn't work for it."],
    ["Добавить в «Мои сайты»", "Add to “My sites”"],
    ["Подбор остановлен", "Search stopped"],
    ["Подбор завершён", "Search finished"],
    ["Все адреса открываются и без обхода: похоже, блокировки сейчас нет.", "All addresses open without bypass: looks like there's no blocking right now."],
    ["Discord и остальные: «{}» — {} из {}.", "Discord and others: “{}”, {} of {}."],
    ["Лучшая: «{}» — {} из {}.", "Best: “{}”, {} of {}."],
    ["YouTube: «{}» — {} из {}.", "YouTube: “{}”, {} of {}."],
    ["Для YouTube ни одна стратегия не сработала. Попробуйте выключить QUIC и повторить подбор.",
      "No strategy worked for YouTube. Try turning off QUIC and searching again."],
    ["Ни одна стратегия не помогла. Попробуйте выключить QUIC, проверить антивирус или задать свои параметры.",
      "No strategy helped. Try turning off QUIC, checking your antivirus or setting custom options."],
    ["Откроет на GitHub заполненное сообщение: провайдер и стратегии. Так другие пользователи с тем же провайдером получат подсказку",
      "Opens a prefilled message on GitHub with your ISP and strategies, so other users with the same ISP get a hint"],
    ["Поделиться результатом", "Share result"],
    ["Стратегии применены", "Strategies applied"],
    ["неизвестен", "unknown"],
    ["Провайдер:", "ISP:"],
    ["Для него обычно работает «{}»", "“{}” usually works for it"],
    ["Применить", "Apply"],
    ["Выбрана рекомендуемая стратегия", "The recommended strategy is selected"],
    ["Применена стратегия для вашего провайдера", "Applied the strategy for your ISP"],
    ["Проверяем: {}", "Checking: {}"],

    // списки сайтов
    ["По одному домену в строке. Поддомены учитываются автоматически:", "One domain per line. Subdomains are included automatically:"],
    ["включает", "includes"],
    ["Открыть папку", "Open folder"],
    ["Режим «Мои сайты»", "“My sites” mode"],
    ["Как выбирать сайты, к которым применяется обход", "How to choose the sites the bypass applies to"],
    ["Только список", "List only"],
    ["Список + авто", "List + auto"],
    ["Все сайты", "All sites"],
    ["Добавить в «Мои сайты»: вставьте ссылку или домен", "Add to “My sites”: paste a link or domain"],
    ["Исключения", "Exclusions"],
    ["Автосписок", "Auto list"],
    ["Сайты, к которым применяется обход, когда включён сервис «Мои сайты».", "Sites the bypass applies to when the “My sites” service is on."],
    ["К этим сайтам обход не применяется никогда. Особенно важно в режиме «Все сайты».", "The bypass never applies to these sites. This matters most in “All sites” mode."],
    ["Сайты, найденные автоматически в режиме «Список + авто»: после нескольких неудачных подключений домен добавляется сюда.",
      "Sites found automatically in “List + auto” mode: after several failed connections the domain is added here."],
    ["Встроенный список, обновляется автоматически с GitHub. Свои сайты добавляйте в «Мои сайты».",
      "Built-in list, updated automatically from GitHub. Add your own sites to “My sites”."],
    ["{} · есть несохранённые изменения", "{} · unsaved changes"],
    ["Есть несохранённые изменения. Отбросить их?", "There are unsaved changes. Discard them?"],
    ["Список сохранён, обход перезапущен", "List saved, bypass restarted"],
    ["Список сохранён", "List saved"],
    ["Очистить автоматически найденные сайты?", "Clear the automatically found sites?"],

    // журнал
    ["Очистить", "Clear"],
    ["Сообщения приложения и winws2. Подробный вывод включается в настройках.", "Messages from the app and winws2. Verbose output can be turned on in Settings."],
    ["Автопрокрутка", "Auto-scroll"],
    ["Журнал скопирован", "Log copied"],

    // настройки
    ["Оформление", "Appearance"],
    ["Тема", "Theme"],
    ["Тёмная, светлая или как в Windows", "Dark, light or same as Windows"],
    ["Как в системе", "System"],
    ["Светлая", "Light"],
    ["Тёмная", "Dark"],
    ["Язык", "Language"],
    ["Язык интерфейса. Журнал всегда на русском", "Interface language. The log is always in Russian"],
    ["Анимации", "Animations"],
    ["Выключите на слабом компьютере или ноутбуке, чтобы интерфейс меньше нагружал видеокарту", "Turn off on a slow computer or laptop to reduce the load on the graphics card"],
    ["Запуск", "Startup"],
    ["Запускать вместе с Windows", "Start with Windows"],
    ["Через Планировщик заданий, без запроса прав при каждом входе", "Via Task Scheduler, without an admin prompt at every sign-in"],
    ["Запускать свёрнутым в трей", "Start minimized to the tray"],
    ["При автозапуске окно не показывается", "The window isn't shown at autostart"],
    ["Включать обход при запуске", "Turn on the bypass at startup"],
    ["Сразу после старта программы", "Right after the app starts"],
    ["Сворачивать в трей при закрытии", "Minimize to the tray on close"],
    ["Обход продолжит работать в фоне, управлять им можно из мини-окна в трее", "The bypass keeps running in the background; control it from the tray mini window"],
    ["Обновления", "Updates"],
    ["Стратегии и списки", "Strategies and lists"],
    ["Из репозитория на GitHub", "From the GitHub repository"],
    ["Версия", "Version"],
    ["Новые стратегии и списки сайтов приходят без переустановки.", "New strategies and site lists arrive without reinstalling."],
    ["Обновить", "Update"],
    ["Автоматически", "Automatically"],
    ["winws2, Lua-скрипты, WinDivert", "winws2, Lua scripts, WinDivert"],
    ["Из релизов bol-van/zapret2.", "From bol-van/zapret2 releases."],
    ["Приложение", "App"],
    ["Из релизов raiyaxkrash/zapret2-rust.", "From raiyaxkrash/zapret2-rust releases."],
    ["Установить", "Install"],
    ["Сторож", "Watchdog"],
    ["Следить за работой обхода", "Watch the bypass"],
    ["Раз в 15 минут проверять, открываются ли сервисы. Если провайдер поменял блокировку, Zapret2 это заметит",
      "Check every 15 minutes that services open. If the ISP changes its blocking, Zapret2 will notice"],
    ["Подбирать стратегию автоматически", "Find a new strategy automatically"],
    ["Когда сервис перестал открываться, сразу подобрать и применить новую стратегию. Иначе — только предупредить",
      "When a service stops opening, find and apply a new strategy right away. Otherwise just show a warning"],
    ["Дополнительно", "Advanced"],
    ["Подробный журнал", "Verbose log"],
    ["winws2 будет писать отладочный вывод (--debug). Немного нагружает процессор", "winws2 writes debug output (--debug). Adds some CPU load"],
    ["Источник обновлений приложения", "App update source"],
    ["Адрес файла latest.json", "URL of latest.json"],
    ["Мастер настройки", "Setup wizard"],
    ["Заново выбрать, что не открывается, и подобрать стратегию", "Choose again what doesn't open and find a strategy"],
    ["Открыть", "Open"],
    ["Папка с данными", "Data folder"],
    ["Zapret2 GUI использует", "Zapret2 GUI uses"],
    ["(bol-van, MIT), драйвер WinDivert. Прокси для Telegram — собственная реализация протокола",
      "(bol-van, MIT) and the WinDivert driver. The Telegram proxy is an independent implementation of the protocol from"],
    ["(Flowseal, MIT); как запасной путь он использует Cloudflare-серверы проекта tg-ws-proxy. Антивирусы иногда ошибочно считают WinDivert опасным: это драйвер перехвата пакетов, а не вирус. Исходный код:",
      "(Flowseal, MIT); as a fallback it uses the Cloudflare servers of the tg-ws-proxy project. Antivirus software sometimes wrongly flags WinDivert as dangerous: it is a packet capture driver, not a virus. Source code:"],
    ["Стратегии и списки обновлены (версия {})", "Strategies and lists updated (version {})"],
    ["Доступна версия {} от {}", "Version {} from {} is available"],
    ["Установлена последняя версия ({})", "The latest version is installed ({})"],
    ["Установлена последняя версия", "The latest version is installed"],
    ["zapret2 обновлён до {}", "zapret2 updated to {}"],
    ["Источник обновлений не настроен.", "The update source isn't set."],
    ["Доступна версия {}. Нажмите «Обновить» вверху окна.", "Version {} is available. Click “Update” at the top of the window."],
    ["Доступна версия {}", "Version {} is available"],
    ["Загрузка обновления…", "Downloading the update…"],
    ["Загрузка: {}%", "Downloading: {}%"],
    ["Загрузка {}%", "Downloading {}%"],
    ["Загружено {} КБ", "Downloaded {} KB"],
    ["Загрузка…", "Downloading…"],
    ["Адрес должен начинаться с https://", "The URL must start with https://"],
    ["Источник обновлений сохранён", "Update source saved"],
    ["Обновить до {}", "Update to {}"],
    ["Что нового:\n{}", "What's new:\n{}"],
    ["Включить режим тестовой подписи драйверов?\n\nЭто нужно, чтобы Windows на ARM64 загрузила неподписанный драйвер WinDivert.\n• в углу рабочего стола появится надпись «Тестовый режим»;\n• Windows разрешит загрузку неподписанных драйверов;\n• выключить потом можно командой bcdedit /set {current} testsigning off.\n\nПосле включения нужна перезагрузка.",
      "Turn on driver test signing mode?\n\nWindows on ARM64 needs it to load the unsigned WinDivert driver.\n• a “Test Mode” label appears in the corner of the desktop;\n• Windows allows loading unsigned drivers;\n• you can turn it off later with bcdedit /set {current} testsigning off.\n\nA restart is required afterwards."],

    // сторож
    ["{}: перестали открываться, Zapret2 подобрал новую стратегию", "{}: stopped opening, Zapret2 found a new strategy"],
    ["перестали открываться, и автоподбор не нашёл рабочую стратегию. Попробуйте выключить QUIC или проверить сайт вручную.",
      "stopped opening and the automatic search found no working strategy. Try turning off QUIC or checking the site manually."],
    ["перестали открываться: похоже, провайдер изменил блокировку.", "stopped opening: looks like the ISP changed its blocking."],
    ["Подобрать заново", "Find again"],
    ["Скрыть", "Hide"],

    // мастер первого запуска
    ["Что у вас не открывается?", "What doesn't open for you?"],
    ["Отметьте нужное, и Zapret2 сам подберёт настройки. Это займёт около минуты.", "Tick what you need and Zapret2 will find the settings itself. It takes about a minute."],
    ["Пропустить", "Skip"],
    ["Настроить", "Set up"],
    ["Настраиваем…", "Setting up…"],
    ["Готово", "Done"],
    ["Сайт, видео и превью", "Website, videos and thumbnails"],
    ["Сайт, приложение и голосовые каналы", "Website, app and voice channels"],
    ["Приложение Telegram Desktop", "Telegram Desktop app"],
    ["Instagram и Facebook", "Instagram and Facebook"],
    ["Нейросети", "AI"],
    ["ChatGPT, Claude, Gemini — через WARP", "ChatGPT, Claude, Gemini via WARP"],
    ["Другие сайты и игры", "Other sites and games"],
    ["Грузятся бесконечно или обрываются", "Load forever or break off"],
    ["Применяем настройки…", "Applying settings…"],
    ["Определяем провайдера…", "Detecting your ISP…"],
    ["Пробуем стратегию для вашего провайдера…", "Trying the strategy for your ISP…"],
    ["Подбираем стратегию…", "Finding a strategy…"],
    ["Каждая стратегия проверяется по очереди", "Each strategy is checked in turn"],
    ["Проверяем результат…", "Checking the result…"],
    ["Готово, всё открывается", "Done, everything opens"],
    ["Готово, но не всё открывается", "Done, but not everything opens"],
    ["Обход включён и будет запускаться вместе с программой.", "The bypass is on and will start with the app."],
    ["Попробуйте выключить QUIC на странице «Стратегии» или добавить проблемные сайты в «Мои сайты».",
      "Try turning off QUIC on the “Strategies” page or adding the problem sites to “My sites”."],
    ["Не получилось", "Didn't work"],
    ["Для Telegram нажмите «Подключить в Telegram» и подтвердите в приложении.", "For Telegram, click “Connect in Telegram” and confirm in the app."],

    // мини-окно трея
    ["Открыть главное окно", "Open main window"],
    ["Стратегия", "Strategy"],
    ["Голос", "Voice"],
    ["Подобрать стратегию", "Find a strategy"],
    ["Выйти из Zapret2", "Quit Zapret2"],

    // стратегии, наборы и WARP-наборы из presets.json
    ["Фейк + наложение (badseq)", "Fake + overlap (badseq)"],
    ["Часто лучший вариант для YouTube: фейк с неверным ACK, затем первый сегмент с наложенным фейковым ClientHello от www.google.com.",
      "Often the best choice for YouTube: a fake with a wrong ACK, then the first segment overlaid with a fake ClientHello from www.google.com."],
    ["Фейк + разрезание (MD5)", "Fake + split (MD5)"],
    ["Универсальный вариант: перед настоящим запросом отправляется фейковый TLS ClientHello с подписью TCP MD5, затем сам запрос режется на части.",
      "All-round option: a fake TLS ClientHello with a TCP MD5 signature is sent before the real request, then the request itself is split into parts."],
    ["Фейк + обратный порядок (badseq)", "Fake + reverse order (badseq)"],
    ["Фейк с неверным ACK (аналог badseq), части запроса отправляются в обратном порядке.", "A fake with a wrong ACK (like badseq); parts of the request are sent in reverse order."],
    ["Фейк + разрезание (timestamp)", "Fake + split (timestamp)"],
    ["Фейк с испорченной TCP timestamp. Часто работает там, где MD5 режется провайдером.", "A fake with a broken TCP timestamp. Often works where the ISP strips MD5."],
    ["Наложение последовательности (seqovl)", "Sequence overlap (seqovl)"],
    ["Без отдельных фейков: в первый сегмент подмешиваются ложные данные, которые сервер отбросит, а DPI примет. Мягкий и быстрый способ.",
      "No separate fakes: junk data the server discards but DPI accepts is mixed into the first segment. Gentle and fast."],
    ["Фейки с авто-TTL", "Fakes with auto TTL"],
    ["Фейки с TTL, рассчитанным так, чтобы они дошли до DPI, но не до сервера. Плюс разрезание со смешиванием фейков.",
      "Fakes with a TTL calculated to reach DPI but not the server, plus splitting mixed with fakes."],
    ["Фейк + перемешивание (badsum)", "Fake + shuffle (badsum)"],
    ["Фейки с неверной контрольной суммой и перемешанный порядок частей.", "Fakes with a wrong checksum and shuffled order of parts."],
    ["Только разрезание", "Split only"],
    ["Без фейков, запрос режется на множество частей. Самый безобидный вариант, помогает против простого DPI.",
      "No fakes, the request is split into many parts. The most harmless option; helps against simple DPI."],
    ["Свои параметры", "Custom options"],
    ["Параметры, заданные вручную.", "Options set by hand."],
    ["Сайт, веб-версия, ссылки t.me. Само приложение использует свой протокол: если оно не подключается, включите в нём прокси",
      "Website, web version, t.me links. The app itself uses its own protocol: if it won't connect, turn on the proxy for it"],
    ["Сайт, веб-версия, ссылки t.me", "Website, web version, t.me links"],
    ["Instagram, Facebook, WhatsApp, Threads", "Instagram, Facebook, WhatsApp, Threads"],
    ["x.com, twitter.com, картинки и видео", "x.com, twitter.com, images and videos"],
    ["Торрент-трекеры", "Torrent trackers"],
    ["RuTracker, Rutor, NNM-Club, Kinozal и другие", "RuTracker, Rutor, NNM-Club, Kinozal and others"],
    ["Сайт и игровые серверы Roblox", "Roblox website and game servers"],
    ["Мессенджер Signal", "Signal messenger"],
    ["Игры и лаунчеры", "Games and launchers"],
    ["Steam, Epic Games, Riot, Ubisoft, EA, GOG, FACEIT: магазины, обновления, сообщества", "Steam, Epic Games, Riot, Ubisoft, EA, GOG, FACEIT: stores, updates, communities"],
    ["Стримы, чат и превью Twitch", "Twitch streams, chat and thumbnails"],
    ["Зарубежные хостинги и CDN", "Foreign hosting and CDNs"],
    ["Сайты, игры и сервисы на Cloudflare, Hetzner, DigitalOcean, OVH, AWS и других хостингах, которые обрываются или грузятся бесконечно",
      "Sites, games and services on Cloudflare, Hetzner, DigitalOcean, OVH, AWS and other hosts that break off or load forever"],
    ["Сайты, игры и сервисы на Cloudflare, Hetzner, DigitalOcean, OVH, AWS и других хостингах", "Sites, games and services on Cloudflare, Hetzner, DigitalOcean, OVH, AWS and other hosts"],
    ["Сайт и картинки LinkedIn", "LinkedIn website and images"],
    ["ChatGPT, Claude, Gemini, Copilot, Perplexity, Grok, Midjourney и другие", "ChatGPT, Claude, Gemini, Copilot, Perplexity, Grok, Midjourney and others"],
    ["Музыка", "Music"],
    ["Spotify и TIDAL", "Spotify and TIDAL"],
    ["Программы и разработка", "Software and development"],
    ["Загрузки и аккаунты Intel, AMD, Autodesk, Adobe, JetBrains, Docker, HashiCorp, Notion", "Downloads and accounts for Intel, AMD, Autodesk, Adobe, JetBrains, Docker, HashiCorp, Notion"],
    ["Меньше нагрузка на видеокарту", "Less load on the graphics card"],
    ["YouTube: {}", "YouTube: {}"],
    ["Универсальный вариант: фейковый ClientHello с подписью TCP MD5, затем запрос режется на части.",
      "All-round option: a fake ClientHello with a TCP MD5 signature, then the request is split into parts."],
    ["Без отдельных фейков. Мягкий и быстрый способ.", "No separate fakes. Gentle and fast."],

    // сообщения приложения (Rust)
    ["Не удалось удалить задание автозапуска", "Couldn't remove the autostart task"],
    ["Не удалось определить пользователя", "Couldn't determine the user"],
    ["Не удалось создать задание автозапуска: {}", "Couldn't create the autostart task: {}"],
    ["Не удалось загрузить {}: {}", "Couldn't load {}: {}"],
    ["Ошибка в загруженном presets.json: {}", "Error in the downloaded presets.json: {}"],
    ["Загруженный список {} пуст", "The downloaded list {} is empty"],
    ["Не удалось сохранить обновление: {}", "Couldn't save the update: {}"],
    ["Интернет", "Internet"],
    ["Не открывается даже ya.ru", "Even ya.ru doesn't open"],
    ["Проверьте подключение к интернету, затем повторите диагностику.", "Check your internet connection, then run diagnostics again."],
    ["Подключение есть ({} мс)", "Connected ({} ms)"],
    ["Не удалось сравнить с DNS-over-HTTPS: сервисы Cloudflare и Google DNS недоступны", "Couldn't compare with DNS-over-HTTPS: Cloudflare and Google DNS are unreachable"],
    ["Подмены адресов не найдено", "No address spoofing found"],
    ["Провайдер подменяет адреса: {}", "Your ISP spoofs addresses: {}"],
    ["Включите DNS-over-HTTPS: Параметры Windows → Сеть и Интернет → свойства подключения → DNS → 1.1.1.1 с шифрованием HTTPS.",
      "Turn on DNS-over-HTTPS: Windows Settings → Network & internet → connection properties → DNS → 1.1.1.1 with HTTPS encryption."],
    ["Открывается с обходом ({} мс)", "Opens with bypass ({} ms)"],
    ["Открывается без обхода ({} мс)", "Opens without bypass ({} ms)"],
    ["Адрес не определяется", "The address doesn't resolve"],
    ["Похоже на блокировку через DNS: включите DNS-over-HTTPS.", "Looks like DNS blocking: turn on DNS-over-HTTPS."],
    ["Сервер не отвечает: похоже на блокировку по IP-адресу", "The server doesn't respond: looks like IP blocking"],
    ["Обход по DPI тут не поможет. Попробуйте WARP или проверьте, работает ли сервис сейчас.", "A DPI bypass won't help here. Try WARP or check whether the service is up right now."],
    ["Соединение обрывается ({}): так работает система DPI", "The connection breaks off ({}): that's how DPI works"],
    ["Текущая стратегия не помогает: подберите стратегию заново на странице «Стратегии».", "The current strategy doesn't help: find a new one on the “Strategies” page."],
    ["Включите обход: это как раз его случай.", "Turn on the bypass: this is exactly what it's for."],
    ["ChatGPT и другие нейросети", "ChatGPT and other AI services"],
    ["Сервис сам закрыл доступ из вашего региона", "The service itself blocks access from your region"],
    ["WARP включён: проверьте, что набор «Нейросети» выбран на странице WARP. Эта проверка идёт напрямую, а браузер пойдёт через WARP.",
      "WARP is on: make sure the “AI” pack is selected on the WARP page. This check goes direct, while the browser goes through WARP."],
    ["Обход DPI тут не поможет: нужен иностранный IP-адрес. Включите WARP.", "A DPI bypass won't help here: you need a foreign IP address. Turn on WARP."],
    ["Доступ из вашего региона не ограничен", "Access from your region isn't restricted"],
    ["Не удалось проверить", "Couldn't check"],
    ["Работает: адрес {} ({})", "Working: address {} ({})"],
    ["Ответ получен, но WARP не активен ({})", "Got a response, but WARP isn't active ({})"],
    ["Выключите и включите WARP на странице WARP.", "Turn WARP off and on again on the WARP page."],
    ["Возможно, провайдер блокирует WARP. Проверьте, что включена «Маскировка» на странице WARP.", "Your ISP may be blocking WARP. Make sure “Obfuscation” is on on the WARP page."],
    ["Другие программы обхода", "Other bypass programs"],
    ["Запущены: {}", "Running: {}"],
    ["Они используют тот же драйвер и мешают Zapret2. Закройте их кнопкой «Завершить» на главной.", "They use the same driver and get in the way of Zapret2. Close them with the “Close them” button on the Home page."],
    ["Ещё не загружен", "Not downloaded yet"],
    ["Он скачается автоматически при включении обхода.", "It is downloaded automatically when you turn on the bypass."],
    ["Версия {}, обход включён", "Version {}, bypass is on"],
    ["Версия {}, обход выключен", "Version {}, bypass is off"],
    ["Сейчас идёт подбор: результаты могут быть неточными", "A strategy search is running: results may be inaccurate"],
    ["Не найден {}", "{} not found"],
    ["Не удалось запустить winws2: {}", "Couldn't start winws2: {}"],
    ["winws2 неожиданно завершился (код {}). Подробности в журнале.", "winws2 exited unexpectedly (code {}). See the log for details."],
    ["Не удалось определить провайдера ({})", "Couldn't detect your ISP ({})"],
    ["адрес", "address"],
    ["сеть", "network"],
    ["Стратегия для AS{} {}", "Strategy for AS{} {}"],
    ["как основная", "same as main"],
    ["Стратегия «{}» не найдена", "Strategy “{}” not found"],
    ["Загрузка zapret2 {}…", "Downloading zapret2 {}…"],
    ["Установка zapret2…", "Installing zapret2…"],
    ["zapret2 {} установлен", "zapret2 {} installed"],
    ["Не удалось загрузить zapret2: {}", "Couldn't download zapret2: {}"],
    ["Уже запущены другие программы обхода ({}). Закройте их кнопкой «Завершить» на главном экране.", "Other bypass programs are already running ({}). Close them with the “Close them” button on the Home page."],
    ["winws2 не запустился. {}", "winws2 didn't start. {}"],
    ["Последние строки журнала:\n{}", "Last log lines:\n{}"],
    ["winws2 несколько раз неожиданно завершился. Обход выключен, подробности в журнале.", "winws2 exited unexpectedly several times. The bypass is off; see the log for details."],
    ["Стратегии и списки актуальны (версия {})", "Strategies and lists are up to date (version {})"],
    ["Стратегии и списки обновлены до версии {}", "Strategies and lists updated to version {}"],
    ["Дождитесь окончания подбора стратегии", "Wait for the strategy search to finish"],
    ["Порт прокси для Telegram должен быть от 1024 до 65535", "The Telegram proxy port must be from 1024 to 65535"],
    ["Неизвестный список", "Unknown list"],
    ["Встроенные списки обновляются вместе с приложением. Добавляйте сайты в «Мои сайты».", "Built-in lists are updated with the app. Add sites to “My sites”."],
    ["Не удалось сохранить список: {}", "Couldn't save the list: {}"],
    ["{} уже есть в списке", "{} is already in the list"],
    ["{} добавлен, сервис «Мои сайты» включён", "{} added, the “My sites” service is on"],
    ["{} добавлен", "{} added"],
    ["Тестовый режим включён. Перезагрузите компьютер, после этого обход заработает.", "Test mode is on. Restart the computer and the bypass will work."],
    ["Сейчас", "Now"],
    ["Недопустимая ссылка", "Invalid link"],
    ["Ссылка на прокси ещё не готова", "The proxy link isn't ready yet"],
    ["Не удалось открыть Telegram: {}", "Couldn't open Telegram: {}"],
    ["Установлена последняя версия zapret2 ({})", "The latest zapret2 version is installed ({})"],
    ["Сейчас идёт подбор стратегии, попробуйте позже", "A strategy search is running, try again later"],
    ["Установка…", "Installing…"],
    ["zapret2 обновлён до версии {}", "zapret2 updated to version {}"],
    ["Некорректный адрес источника обновлений", "Invalid update source URL"],
    ["Не удалось проверить обновления: {}", "Couldn't check for updates: {}"],
    ["Обновлений нет", "No updates"],
    ["Ошибка загрузки обновления: {}", "Update download error: {}"],
    ["Ошибка установки: {}", "Installation error: {}"],
    ["Не удалось включить обход: {}", "Couldn't turn on the bypass: {}"],
    ["Прокси для Telegram: {}", "Telegram proxy: {}"],
    ["Не найдена переменная LOCALAPPDATA", "The LOCALAPPDATA variable wasn't found"],
    ["Не удалось создать {}: {}", "Couldn't create {}: {}"],
    ["Не удалось скопировать список {}: {}", "Couldn't copy the list {}: {}"],
    ["Не удалось установить ARM64-драйвер WinDivert: {}", "Couldn't install the ARM64 WinDivert driver: {}"],
    ["Не удалось скопировать {}: {}", "Couldn't copy {}: {}"],
    ["Тестовый режим нужен только на компьютерах с ARM64", "Test mode is only needed on ARM64 computers"],
    ["Включена безопасная загрузка (Secure Boot). Выключите её в настройках UEFI/BIOS, затем повторите.", "Secure Boot is on. Turn it off in the UEFI/BIOS settings, then try again."],
    ["Не найден presets.json: {}", "presets.json not found: {}"],
    ["Ошибка в presets.json: {}", "Error in presets.json: {}"],
    ["Список «Мои сайты» пуст. Добавьте домены или выберите режим «Все сайты».", "The “My sites” list is empty. Add domains or choose “All sites” mode."],
    ["Не выбрано ни одного сервиса для обхода.", "No services are selected for the bypass."],
    ["Не удалось сохранить настройки: {}", "Couldn't save the settings: {}"],
    ["обрыв после {} КБ", "cut off after {} KB"],
    ["таймаут", "timeout"],
    ["не удалось подключиться", "couldn't connect"],
    ["Подбор уже выполняется", "A search is already running"],
    ["Проверка без обхода", "Check without bypass"],
    ["системный генератор случайных чисел недоступен", "the system random number generator is unavailable"],
    ["Некорректный секрет прокси", "Invalid proxy secret"],
    ["Порт {} уже занят другой программой (возможно, запущен TgWsProxy). Закройте её или выберите другой порт в настройках. ({})",
      "Port {} is already used by another program (TgWsProxy may be running). Close it or choose another port in the settings. ({})"],
    ["Прокси для Telegram работает на 127.0.0.1:{}", "The Telegram proxy is running on 127.0.0.1:{}"],
    ["Прокси для Telegram остановлен", "The Telegram proxy is stopped"],
    ["Не удалось обновить список запасных серверов: {}", "Couldn't update the fallback server list: {}"],
    ["Подключений: {}, отправлено {}, получено {}", "Connections: {}, sent {}, received {}"],
    ["Подключение с неверным секретом. Подключите прокси в Telegram заново кнопкой «Подключить в Telegram»",
      "Connection with a wrong secret. Connect the proxy in Telegram again with the “Connect in Telegram” button"],
    ["через Cloudflare", "via Cloudflare"],
    ["напрямую по TCP", "direct over TCP"],
    ["нет доступного пути", "no route available"],
    ["список доменов пуст", "the domain list is empty"],
    ["Введите адрес сайта", "Enter a site address"],
    ["Введите домен латиницей. Для доменов .рф используйте punycode (xn--…)", "Enter the domain in Latin letters. For .рф domains use punycode (xn--…)"],
    ["«{}» не похоже на адрес сайта", "“{}” doesn't look like a site address"],
    ["Некорректный ключ WARP", "Invalid WARP key"],
    ["Некорректный ключ сервера WARP", "Invalid WARP server key"],
    ["Сервер WARP недоступен: {}", "The WARP server is unreachable: {}"],
    ["Регистрация WARP не удалась: HTTP {} {}", "WARP registration failed: HTTP {} {}"],
    ["Некорректный ответ WARP: {}", "Invalid WARP response: {}"],
    ["WARP не вернул сервер для подключения", "WARP didn't return a server to connect to"],
    ["Порт {} для WARP занят другой программой ({})", "Port {} for WARP is used by another program ({})"],
    ["Подключение к WARP…", "Connecting to WARP…"],
    ["Связь с WARP пропала, переподключение…", "Lost connection to WARP, reconnecting…"],
    ["WARP выключен", "WARP is off"],
    ["WARP не запущен", "WARP isn't running"],
    ["cloudflare.com: адрес не найден", "cloudflare.com: address not found"],
    ["Туннель WARP остановлен", "The WARP tunnel is stopped"],
    ["Сервер не ответил через WARP", "The server didn't respond through WARP"],
    ["{}: адрес не найден", "{}: address not found"],
    ["Не удалось включить системный прокси для WARP", "Couldn't set the system proxy for WARP"],
    ["{}: таймаут подключения через WARP", "{}: connection timeout through WARP"],
    ["{}: нет ответа", "{}: no response"],
    ["WARP подключён через {}", "WARP connected via {}"],
    ["Не удалось подключиться к WARP ни через одну точку ({}). Возможно, WARP блокируется у вашего провайдера",
      "Couldn't connect to WARP through any endpoint ({}). Your ISP may be blocking WARP"],
    ["сервер не принял соединение", "the server refused the connection"],
    ["Сервисы перестали открываться. Zapret2 подбирает стратегию заново, это займёт около минуты.", "Services stopped opening. Zapret2 is finding a new strategy; it takes about a minute."],
    ["После", "After"],
    ["GitHub недоступен: {}", "GitHub is unreachable: {}"],
    ["Некорректный ответ GitHub: {}", "Invalid GitHub response: {}"],
    ["В релизе нет zip-архива", "The release has no zip archive"],
    ["Ошибка загрузки: {}", "Download error: {}"],
    ["Контрольная сумма архива не совпадает, загрузка отменена", "The archive checksum doesn't match, download cancelled"],
    ["Не удалось получить контрольные суммы: {}", "Couldn't get checksums: {}"],
    ["Нет контрольных сумм для проверки zapret2, загрузка отменена", "No checksums to verify zapret2, download cancelled"],
    ["Повреждённый архив: {}", "Corrupted archive: {}"],
    ["Контрольная сумма {} не совпадает, загрузка отменена", "Checksum of {} doesn't match, download cancelled"],
    ["Для {} нет контрольной суммы, загрузка отменена", "No checksum for {}, download cancelled"],
    ["В архиве не найдены winws2.exe или Lua-скрипты", "winws2.exe or Lua scripts not found in the archive"],
    ["Не удалось установить обновление: {}", "Couldn't install the update: {}"],
    ["TCP: таймаут", "TCP: timeout"],
    ["TLS: таймаут", "TLS: timeout"],
    ["WebSocket: таймаут ответа", "WebSocket: response timeout"],
    ["WebSocket: пустой ответ", "WebSocket: empty response"],
  ];

  /* --------------------------------------------------------------- движок */
  const CYR = /[А-Яа-яЁё]/;
  const exact = new Map();
  const patterns = [];
  const reEsc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  for (const [ru, enText] of D) {
    if (!ru.includes("{}")) { exact.set(ru, enText); continue; }
    // «{current}» в тексте bcdedit — не шаблон: такие строки без {} остаются точными.
    const parts = ru.split("{}");
    const re = new RegExp("^" + parts.map(reEsc).join("([\\s\\S]+?)") + "$");
    const outParts = enText.split("{}");
    patterns.push({ re, outParts, weight: parts.join("").length });
  }
  // Сначала более длинные шаблоны: «Работает · 127.0.0.1:{}» раньше «Работает {}».
  patterns.sort((a, b) => b.weight - a.weight);

  const cache = new Map();
  /** Перевод строки целиком; незнакомое возвращается как есть. */
  function tr(s, depth = 0) {
    if (!s || !CYR.test(s)) return s;
    const hit = exact.get(s);
    if (hit !== undefined) return hit;
    if (depth > 3) return s;
    if (depth === 0 && cache.has(s)) return cache.get(s);
    let out = s;
    for (const p of patterns) {
      const m = p.re.exec(s);
      if (!m) continue;
      const vals = m.slice(1).map((v) => tr(v, depth + 1).replace(/^(\d+),(\d+)$/, "$1.$2"));
      out = p.outParts.reduce((acc, part, i) => acc + vals[i - 1] + part);
      break;
    }
    // Несколько предложений в одном узле: переводим по отдельности.
    if (out === s && depth === 0) {
      const parts = s.split(/(?<=[.!?:])\s+(?=[А-ЯЁA-Z])/);
      if (parts.length > 1) out = parts.map((x) => tr(x, 1)).join(' ');
    }
    if (depth === 0) { if (cache.size > 2000) cache.clear(); cache.set(s, out); }
    return out;
  }

  /** Перевод с сохранением пробелов по краям (у текстовых узлов они значимы). */
  function translate(text) {
    if (!text || !CYR.test(text)) return text;
    const m = /^(\s*)([\s\S]*?)(\s*)$/.exec(text);
    let core = m[2];
    let out = tr(core);
    if (out === core) {
      const flat = core.replace(/\s+/g, " ");
      if (flat !== core) { const f = tr(flat); if (f !== flat) out = f; }
    }
    return out === core ? text : m[1] + out + m[3];
  }

  const SKIP = "#log, pre, .code, textarea, script, style, [data-no-i18n]";
  const ATTRS = ["placeholder", "title", "aria-label"];

  function translateNode(node) {
    if (node.nodeType === 3) {
      const p = node.parentElement;
      if (!p || p.closest(SKIP)) return;
      const v = translate(node.nodeValue);
      if (v !== node.nodeValue) node.nodeValue = v;
      return;
    }
    if (node.nodeType !== 1 || node.closest(SKIP)) return;
    translateAttrs(node);
    const walker = document.createTreeWalker(node, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT, {
      acceptNode: (n) => (n.nodeType === 1 && n.matches(SKIP) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT),
    });
    let n;
    while ((n = walker.nextNode())) {
      if (n.nodeType === 1) translateAttrs(n);
      else { const v = translate(n.nodeValue); if (v !== n.nodeValue) n.nodeValue = v; }
    }
  }
  function translateAttrs(el) {
    for (const a of ATTRS) {
      const v = el.getAttribute(a);
      if (v && CYR.test(v)) { const t2 = translate(v); if (t2 !== v) el.setAttribute(a, t2); }
    }
  }

  function start() {
    document.documentElement.lang = "en";
    document.title = translate(document.title);
    translateNode(document.body);
    new MutationObserver((list) => {
      for (const m of list) {
        if (m.type === "childList") m.addedNodes.forEach(translateNode);
        else if (m.type === "characterData") translateNode(m.target);
        else if (m.type === "attributes") { if (!m.target.closest(SKIP)) translateAttrs(m.target); }
      }
    }).observe(document.body, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ATTRS });
  }
  if (en) {
    if (document.body) start();
    else document.addEventListener("DOMContentLoaded", start);
  }

  return {
    lang,
    en,
    locale: en ? "en-US" : "ru-RU",
    /** Перевод строки для confirm() и других мест вне DOM. */
    t: (s) => (en ? translate(s) : s),
    /** Слово к числу: в русском три формы, в английском две. */
    plural(n, one, few, many) {
      if (en) return tr(n === 1 ? one : many);
      const a = n % 10, b = n % 100;
      return a === 1 && b !== 11 ? one : a >= 2 && a <= 4 && (b < 10 || b >= 20) ? few : many;
    },
    /** Применить язык из настроек ("auto" | "ru" | "en"); systemLang — язык Windows от приложения. */
    sync(setting, systemLang) {
      const want = setting === "ru" || setting === "en" ? setting
        : systemLang === "ru" || systemLang === "en" ? systemLang
        : RU_SYSTEM.test(navigator.language || "ru") ? "ru" : "en";
      if (new URLSearchParams(location.search).get("lang")) return;
      try { if (localStorage.getItem(KEY) === want && want === lang) return; localStorage.setItem(KEY, want); } catch {}
      if (want !== lang) location.reload();
    },
  };
})();
