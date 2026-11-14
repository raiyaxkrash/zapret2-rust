"use strict";
/* Общий код главного окна и мини-окна в трее. */

/* ===================================================================== icons (\u0441\u043E\u0431\u0441\u0442\u0432\u0435\u043D\u043D\u044B\u0439 \u043D\u0430\u0431\u043E\u0440, \u043B\u0438\u043D\u0438\u0438 24\u00D724) */
const ICONS = {
  home: '<path d="M3 10.5 12 3l9 7.5"/><path d="M5 9.5V20a1 1 0 0 0 1 1h4v-6h4v6h4a1 1 0 0 0 1-1V9.5"/>',
  sliders: '<path d="M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0"/><circle cx="16" cy="6" r="2"/><circle cx="10" cy="12" r="2"/><circle cx="18" cy="18" r="2"/>',
  list: '<path d="M9 6h11M9 12h11M9 18h11"/><circle cx="4.5" cy="6" r="1"/><circle cx="4.5" cy="12" r="1"/><circle cx="4.5" cy="18" r="1"/>',
  terminal: '<rect x="3" y="4" width="18" height="16" rx="3"/><path d="m7 9 3 3-3 3M13 15h4"/>',
  gear: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/>',
  power: '<path d="M12 3v8"/><path d="M6.3 6.8a8 8 0 1 0 11.4 0"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  play: '<rect x="2.5" y="5" width="19" height="14" rx="4.5"/><path d="m10 9 5 3-5 3z" fill="currentColor"/>',
  chat: '<path d="M8 17H6a3 3 0 0 1-3-3V7a3 3 0 0 1 3-3h12a3 3 0 0 1 3 3v7a3 3 0 0 1-3 3h-5l-5 4z"/><circle cx="9" cy="10.5" r="1" fill="currentColor"/><circle cx="15" cy="10.5" r="1" fill="currentColor"/>',
  mic: '<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/>',
  globe: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"/>',
  wand: '<path d="m15 4 1 2 2 1-2 1-1 2-1-2-2-1 2-1zM5 19 16 8"/><path d="M19 13l.6 1.4L21 15l-1.4.6L19 17l-.6-1.4L17 15l1.4-.6z"/>',
  code: '<path d="m8 8-4 4 4 4M16 8l4 4-4 4M14 5l-4 14"/>',
  copy: '<rect x="8" y="8" width="12" height="12" rx="2.5"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/>',
  folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  trash: '<path d="M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3"/>',
  box: '<path d="M21 8 12 3 3 8v8l9 5 9-5z"/><path d="m3 8 9 5 9-5M12 13v8"/>',
  refresh: '<path d="M20 11a8 8 0 0 0-14.5-4.5L4 8M4 4v4h4M4 13a8 8 0 0 0 14.5 4.5L20 16M20 20v-4h-4"/>',
  sync: '<path d="M20 11a8 8 0 0 0-14.5-4.5L4 8M4 4v4h4M4 13a8 8 0 0 0 14.5 4.5L20 16M20 20v-4h-4"/>',
  check: '<circle cx="12" cy="12" r="9"/><path d="m8 12 3 3 5-6"/>',
  alert: '<path d="M12 3 2 20h20z"/><path d="M12 10v4M12 17.5v.01"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7.5v.01"/>',
  x: '<path d="M6 6l12 12M18 6 6 18"/>',
  min: '<path d="M6 12h12"/>',
  max: '<rect x="6" y="6" width="12" height="12" rx="2"/>',
  restore: '<rect x="5" y="8" width="11" height="11" rx="2"/><path d="M8 8V6a1 1 0 0 1 1-1h9a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1h-2"/>',
  close: '<path d="M7 7l10 10M17 7 7 17"/>',
  open: '<path d="M14 4h6v6M20 4l-9 9"/><path d="M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4"/>',
  color: '<path d="M12 3a9 9 0 1 0 0 18c1.2 0 2-.8 2-2 0-.5-.2-1-.5-1.3-.3-.4-.5-.8-.5-1.3 0-1.1.9-2 2-2H17a4 4 0 0 0 4-4c0-4.4-4-7.4-9-7.4z"/><circle cx="7.5" cy="11" r="1.2" fill="currentColor"/><circle cx="10" cy="7" r="1.2" fill="currentColor"/><circle cx="15" cy="7.5" r="1.2" fill="currentColor"/>',
  download: '<path d="M12 4v11M7 10l5 5 5-5M5 20h14"/>',
  rocket: '<path d="M5 15c-1.5 1.5-2 5-2 5s3.5-.5 5-2"/><path d="M9 15 6 12l3-5c3-4 8-5 12-4 1 4 0 9-4 12l-5 3z"/><circle cx="15" cy="9" r="1.6"/>',
  flash: '<path d="M13 2 4 14h7l-1 8 9-12h-7z"/>',
  chevron: '<path d="m9 6 6 6-6 6"/>',
  exit: '<path d="M7 7l10 10M17 7 7 17"/>',
  send: '<path d="M21 3 3 10.5l6.5 2.5L12 20l3-4.5L21 3z"/><path d="m9.5 13 5-4"/>',
  camera: '<rect x="3" y="3" width="18" height="18" rx="5.5"/><circle cx="12" cy="12" r="4"/><circle cx="17.3" cy="6.7" r="1" fill="currentColor"/>',
  hash: '<path d="M5 6l14 12M19 6 5 18"/>',
  gamepad: '<path d="M7 8h10a4 4 0 0 1 4 4v1.5a3.5 3.5 0 0 1-6.2 2.2L14 15h-4l-.8.7A3.5 3.5 0 0 1 3 13.5V12a4 4 0 0 1 4-4z"/><path d="M8 11v3M6.5 12.5h3"/><circle cx="16" cy="11.5" r="1" fill="currentColor"/><circle cx="17.5" cy="13.5" r="1" fill="currentColor"/>',
  briefcase: '<rect x="3" y="7" width="18" height="13" rx="3"/><path d="M9 7V5a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v2M3 13h18"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
  server: '<rect x="3" y="4" width="18" height="7" rx="2"/><rect x="3" y="13" width="18" height="7" rx="2"/><path d="M7 7.5h.01M7 16.5h.01M11 7.5h6M11 16.5h6"/>',
  tick: '<path d="m5 12.5 4.5 4.5L19 7.5"/>',
  chip: '<rect x="6" y="6" width="12" height="12" rx="2"/><path d="M9 2v4M15 2v4M9 18v4M15 18v4M2 9h4M2 15h4M18 9h4M18 15h4"/>',
};
function iconSvg(name) {
  return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">${ICONS[name] || ""}</svg>`;
}
function icon(name) {
  return `<span class="ico" aria-hidden="true">${iconSvg(name)}</span>`;
}
function hydrateIcons(root = document) {
  root.querySelectorAll("[data-icon]").forEach((el) => {
    if (!el.dataset.done) {
      el.classList.add("ico");
      el.innerHTML = iconSvg(el.dataset.icon);
      el.dataset.done = "1";
    }
  });
}

/* ===================================================================== bridge */
const TAURI = window.__TAURI__;
const invoke = TAURI ? TAURI.core.invoke : (...a) => mockInvoke(...a);
const listen = TAURI ? TAURI.event.listen : () => Promise.resolve();
const appWindow = TAURI?.window?.getCurrentWindow ? TAURI.window.getCurrentWindow() : null;

/* ===================================================================== helpers */
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
const errText = (e) => (typeof e === "string" ? e : e?.message || JSON.stringify(e));

const plural = (n, one, few, many) => I18N.plural(n, one, few, many);
function fmtUptime(sec) {
  const h = Math.floor(sec / 3600), m = Math.floor((sec % 3600) / 60), s = sec % 60;
  return h ? `${h} ч ${m} мин` : m ? `${m} мин ${s} с` : `${s} с`;
}

/* ===================================================================== appearance */
const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");
let appearance = { theme: "system", platform: null };

/** Тема ("system" | "light" | "dark") и анимации. */
function applyAppearance(theme, platform, animations) {
  if (theme) appearance.theme = theme;
  if (platform) appearance.platform = platform;
  if (animations !== undefined) appearance.animations = animations !== false;
  const dark = appearance.theme === "dark" || (appearance.theme === "system" && darkQuery.matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  document.documentElement.classList.toggle("no-anim", appearance.animations === false);
}
darkQuery.addEventListener("change", () => applyAppearance());

/* Демо-режим для скриншотов (?demo=1&w=…&h=…, только в браузере без Tauri): фиксированный размер и без анимаций. */
(() => {
  const q = new URLSearchParams(location.search);
  if (TAURI || !q.get("demo")) return;
  const root = document.documentElement;
  root.classList.add("no-anim");
  if (q.get("w")) {
    const css = `html,body{width:${+q.get("w")}px!important;height:${+q.get("h")}px!important;overflow:hidden!important}
      .window{height:${+q.get("h")}px!important}.log{height:auto}#listEditor{height:300px}`;
    document.head.insertAdjacentHTML("beforeend", `<style>${css}</style>`);
  }
})();

/* ===================================================================== видимость окна */
/* Когда окно скрыто в трей или свёрнуто, останавливаем анимации и периодические обновления. */
// Флаг от приложения (окно спрятано в трей); свёрнутое окно определяется через document.hidden.
let windowShown = true;
const visibilityHandlers = [];
function isActive() { return windowShown && !document.hidden; }
function onVisible(fn) { visibilityHandlers.push(fn); }
function syncVisibility() {
  const active = isActive();
  document.documentElement.classList.toggle("paused", !active);
  if (active) visibilityHandlers.forEach((fn) => fn());
}
document.addEventListener("visibilitychange", syncVisibility);
listen("visibility", ({ payload }) => { windowShown = !!payload; syncVisibility(); });
syncVisibility();

/** Периодическая задача, которая работает только пока окно видно. */
function everyWhileVisible(ms, fn) {
  setInterval(() => { if (isActive()) fn(); }, ms);
}

/* ===================================================================== значок обновления */
let updateInstalling = false;
function renderUpdatePill(upd) {
  const pill = $("#updatePill");
  if (!pill) return;
  pill.hidden = !upd;
  if (upd && !updateInstalling) {
    $("#updatePillText").textContent = `Обновить до ${upd.latest}`;
    pill.title = upd.notes ? `Что нового:\n${upd.notes}` : "Установить обновление";
  }
  const badge = $("#settingsBadge");
  if (badge) badge.hidden = !upd;
}
async function installAppUpdate() {
  if (updateInstalling) return;
  updateInstalling = true;
  const pill = $("#updatePill");
  if (pill) { pill.disabled = true; $("#updatePillText").textContent = "Загрузка…"; }
  try {
    await invoke("install_app_update"); // после установки приложение перезапустится
  } catch (e) {
    updateInstalling = false;
    if (pill) { pill.disabled = false; $("#updatePillText").textContent = "Повторить"; }
    toast(errText(e), "err", 9000);
  }
}
$("#updatePill")?.addEventListener("click", installAppUpdate);
listen("app-update-available", ({ payload }) => renderUpdatePill(payload));
listen("app-update-progress", ({ payload: [done, total] }) => {
  if (!updateInstalling || !$("#updatePillText")) return;
  $("#updatePillText").textContent = total ? `Загрузка ${Math.round((done / total) * 100)}%` : "Загрузка…";
});

/* ===================================================================== toasts */
function toast(msg, kind = "ok", ms = 4500) {
  const box = $("#toasts");
  if (!box) return;
  const el = document.createElement("div");
  el.className = `toast ${kind}`;
  el.innerHTML = `${icon(kind === "err" ? "alert" : kind === "info" ? "info" : "check")}<div>${esc(msg)}</div>`;
  box.appendChild(el);
  setTimeout(() => { el.classList.add("hide"); setTimeout(() => el.remove(), 300); }, ms);
}

/* ===================================================================== browser mock (только для предпросмотра без Tauri) */
function mockInvoke(cmd, args) {
  const demo = !!new URLSearchParams(location.search).get("demo");
  const m = (window.__mock ||= {
    running: demo, since: demo ? Math.floor(Date.now() / 1000) - 5025 : null,
    settings: { packs: demo ? ["telegram", "meta"] : [], strategy: demo ? "seqovl" : "fake_multisplit_md5", youtube_strategy: demo ? "fake_multisplit_md5" : "", youtube: true, discord: true, discord_voice: true, custom: false, custom_mode: "list", quic: true, autostart: false, start_minimized: true, engine_on_launch: true, close_to_tray: true, debug_log: false, engine_auto_update: true, content_auto_update: true, app_auto_check: true, app_update_url: "https://github.com/raiyaxkrash/zapret2-rust/releases/latest/download/latest.json", theme: new URLSearchParams(location.search).get("theme") || "system", lang: "auto", custom_tls: "--payload=tls_client_hello --lua-desync=multisplit:pos=1", custom_http: "", custom_quic: "", tg_proxy: demo, tg_proxy_port: 1443, warp: demo, warp_port: 40080, warp_junk: true, warp_packs: ["ai"], warp_custom: [], watchdog: true, watchdog_auto: true, tg_proxy_secret: "0123456789abcdef0123456789abcdef" },
    lists: { user: "# Мои сайты\nrutracker.org\n", exclude: "gosuslugi.ru\n", auto: "", youtube: "youtube.com\ngooglevideo.com\n", discord: "discord.com\n" },
  });
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const strategies = [
    { id: "fake_multisplit_md5", name: "Фейк + разрезание (MD5)", description: "Универсальный вариант: фейковый ClientHello с подписью TCP MD5, затем запрос режется на части.", tls: ["--payload=tls_client_hello", "--lua-desync=fake:blob=tls_google:tcp_md5:repeats=6"], http: [], quic: null },
    { id: "seqovl", name: "Наложение последовательности (seqovl)", description: "Без отдельных фейков. Мягкий и быстрый способ.", tls: ["--lua-desync=multisplit:pos=1:seqovl=681"], http: [], quic: null },
    { id: "custom", name: "Свои параметры", description: "Параметры, заданные вручную.", tls: [], http: [], quic: null },
  ];
  switch (cmd) {
    case "get_state": return Promise.resolve({
      status: { running: m.running, since: m.since, last_error: null, command_line: "", pid: null },
      settings: m.settings, engine_version: "1.0.7", app_version: "2.1.1", conflicts: [], testing: false,
      data_dir: "C:\\Users\\user\\AppData\\Local\\Zapret2GUI", user_domains: 1, auto_domains: 0, autostart: m.settings.autostart,
      content_version: 2,
      app_update: new URLSearchParams(location.search).get("upd") ? { configured: true, available: true, current: "1.1.0", latest: "1.2.0", notes: "Меньше нагрузка на видеокарту" } : null,
      warp: { running: !!m.settings.warp, connected: !!m.settings.warp, since: null, last_error: null, endpoint: "162.159.192.1:2408", port: 40080, rx: 48234496, tx: 3145728, domains: 23 },
      tg_proxy: { available: true, running: !!m.settings.tg_proxy, since: null, last_error: null, port: m.settings.tg_proxy_port, link: `tg://proxy?server=127.0.0.1&port=${m.settings.tg_proxy_port}&secret=dd${m.settings.tg_proxy_secret}` },
      platform: { arm64: !!new URLSearchParams(location.search).get("arm"), lang: "ru", testsigning: false, secure_boot: false, build: 26100, mica: false, accent: { dark_theme: "#4cc2ff", light_theme: "#005fb8" } },
    });
    case "start_engine": return wait(900).then(() => { m.running = true; m.since = Math.floor(Date.now() / 1000); });
    case "stop_engine": return wait(200).then(() => { m.running = false; });
    case "get_strategies": return Promise.resolve(strategies);
    case "get_warp_packs": return Promise.resolve([
      { id: "ai", name: "Нейросети", icon: "wand", description: "ChatGPT, Claude, Gemini, Copilot, Perplexity, Grok, Midjourney и другие", domains: new Array(23).fill("x") },
      { id: "music", name: "Музыка", icon: "play", description: "Spotify и TIDAL", domains: new Array(5).fill("x") },
      { id: "dev", name: "Программы и разработка", icon: "code", description: "Загрузки и аккаунты Intel, AMD, Autodesk, Adobe, JetBrains, Docker, HashiCorp, Notion", domains: new Array(11).fill("x") },
    ]);
    case "warp_check": return wait(800).then(() => ({ ip: "104.28.207.39", loc: "AT", colo: "VIE", warp: true }));
    case "run_diagnostics": return wait(1500).then(() => [
      { title: "Интернет", status: "good", detail: "Подключение есть (84 мс)", advice: "" },
      { title: "DNS", status: "good", detail: "Подмены адресов не найдено", advice: "" },
      { title: "YouTube", status: "good", detail: "Открывается с обходом (320 мс)", advice: "" },
      { title: "Discord", status: "good", detail: "Открывается с обходом (180 мс)", advice: "" },
      { title: "Instagram", status: "bad", detail: "Соединение обрывается (обрыв после 16 КБ): так работает система DPI", advice: "Текущая стратегия не помогает: подберите стратегию заново на странице «Стратегии»." },
      { title: "ChatGPT и другие нейросети", status: "warn", detail: "Сервис сам закрыл доступ из вашего региона", advice: "Обход DPI тут не поможет: нужен иностранный IP-адрес. Включите WARP." },
      { title: "zapret2", status: "good", detail: "Версия 1.0.7, обход включён", advice: "" },
    ]);
    case "net_traffic": {
      // Демо: плавно меняющаяся скорость около 2–6 МБ/с.
      const t = (window.__traf ||= { rx: 1e9, tx: 1e8, n: 0 });
      t.n++;
      t.rx += (2 + 2 * Math.sin(t.n / 7) + Math.random()) * 1048576 * 2;
      t.tx += (0.2 + 0.1 * Math.sin(t.n / 5)) * 1048576 * 2;
      return Promise.resolve({ rx: Math.round(t.rx), tx: Math.round(t.tx) });
    }
    case "get_isp": return wait(300).then(() => ({ isp: { asn: 12389, name: "PJSC Rostelecom" }, hint: null }));
    case "save_settings": m.settings = args.new; return Promise.resolve(m.settings);
    case "get_command_line": return Promise.resolve("engine\\bin\\winws2.exe --wf-tcp-out=80,443 ...");
    case "read_list": return Promise.resolve(m.lists[args.name] || "");
    case "write_list": m.lists[args.name] = args.content; return Promise.resolve();
    case "get_logs": return Promise.resolve(["[app] Предпросмотр интерфейса без Tauri"]);
    case "run_strategy_test": if (args?.site) return wait(1500).then(() => ({
      site: args.site.replace(/^https?:\/\/(www\.)?/, "").split("/")[0], cancelled: false, best: "seqovl", best_youtube: null,
      baseline: { id: "baseline", ok: 0, total: 1, avg_ms: 0, checks: [], yt_ok: 0, yt_total: 0, other_ok: 0, other_total: 1, error: null },
      results: [
        { id: "fake_multisplit_md5", name: "Фейк + разрезание (MD5)", ok: 0, total: 1, avg_ms: 0, checks: [], yt_ok: 0, yt_total: 0, other_ok: 0, other_total: 1 },
        { id: "seqovl", name: "Наложение последовательности (seqovl)", ok: 1, total: 1, avg_ms: 510, checks: [], yt_ok: 0, yt_total: 0, other_ok: 1, other_total: 1 },
      ],
    }));
    return wait(1500).then(() => ({
      baseline: { id: "baseline", name: "Без обхода", ok: 1, total: 7, avg_ms: 300, checks: [], yt_ok: 0, yt_total: 4, other_ok: 1, other_total: 3 },
      results: [
        { id: "fake_multisplit_md5", name: "Фейк + разрезание (MD5)", ok: 4, total: 7, avg_ms: 420, checks: [], yt_ok: 1, yt_total: 4, other_ok: 3, other_total: 3 },
        { id: "seqovl", name: "Наложение последовательности (seqovl)", ok: 6, total: 7, avg_ms: 380, checks: [], yt_ok: 4, yt_total: 4, other_ok: 2, other_total: 3 },
        { id: "custom", name: "Свои параметры", ok: 2, total: 7, avg_ms: 610, checks: [], yt_ok: 0, yt_total: 4, other_ok: 2, other_total: 3 },
      ],
      best: "fake_multisplit_md5", best_youtube: "seqovl", cancelled: false,
    }));
    case "check_engine_update": return wait(500).then(() => ({ current: "1.0.7", latest: "1.0.7", available: false, published_at: "2026-11-05T08:01:55Z" }));
    case "check_app_update": return Promise.resolve({ configured: true, available: false });
    case "check_content_update": return wait(400).then(() => ({ current: 1, latest: 1, available: false }));
    case "update_content": return wait(400).then(() => "Стратегии и списки актуальны (версия 1)");
    case "get_last_report": return Promise.resolve(null);
    case "get_packs": return Promise.resolve([
      { id: "telegram", name: "Telegram", icon: "send", description: "Сайт, веб-версия, ссылки t.me", domains: 10 },
      { id: "meta", name: "Instagram и Facebook", icon: "camera", description: "Instagram, Facebook, WhatsApp, Threads", domains: 16 },
      { id: "x", name: "X (Twitter)", icon: "hash", description: "x.com, twitter.com, картинки и видео", domains: 7 },
      { id: "torrents", name: "Торрент-трекеры", icon: "download", description: "RuTracker, Rutor, NNM-Club, Kinozal и другие", domains: 11 },
      { id: "roblox", name: "Roblox", icon: "gamepad", description: "Сайт и игровые серверы Roblox", domains: 5 },
      { id: "signal", name: "Signal", icon: "chat", description: "Мессенджер Signal", domains: 7 },
      { id: "games", name: "Игры и лаунчеры", icon: "gamepad", description: "Steam, Epic Games, Riot, Ubisoft, EA, GOG, FACEIT: магазины, обновления, сообщества", domains: 30 },
      { id: "twitch", name: "Twitch", icon: "play", description: "Стримы, чат и превью Twitch", domains: 7 },
      { id: "hosting", name: "Зарубежные хостинги и CDN", icon: "server", kind: "ipset", description: "Сайты, игры и сервисы на Cloudflare, Hetzner, DigitalOcean, OVH, AWS и других хостингах", domains: 16346 },
      { id: "linkedin", name: "LinkedIn", icon: "briefcase", description: "Сайт и картинки LinkedIn", domains: 3 },
    ]);
    case "quick_check": return wait(900).then(() => ({ id: "now", checks: [
      { service: "youtube", ok: true, ms: 320 }, { service: "youtube", ok: true, ms: 410 }, { service: "youtube", ok: false, ms: 0 },
      { service: "discord", ok: true, ms: 180 }, { service: "discord", ok: true, ms: 220 },
    ] }));
    case "add_domain": return wait(300).then(() => `${args.input.replace(/^https?:\/\/(www\.)?/, "").split("/")[0]} добавлен`);
    default: return Promise.resolve(null);
  }
}
