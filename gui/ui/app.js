"use strict";

/* ===================================================================== icons */
const ICONS = {
  home: '<path d="M3 10.5 12 3l9 7.5"/><path d="M5 9.5V20a1 1 0 0 0 1 1h4v-6h4v6h4a1 1 0 0 0 1-1V9.5"/>',
  sliders: '<path d="M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0"/><circle cx="16" cy="6" r="2"/><circle cx="10" cy="12" r="2"/><circle cx="18" cy="18" r="2"/>',
  list: '<path d="M9 6h11M9 12h11M9 18h11"/><circle cx="4.5" cy="6" r="1"/><circle cx="4.5" cy="12" r="1"/><circle cx="4.5" cy="18" r="1"/>',
  terminal: '<rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="m7 9 3 3-3 3M13 15h4"/>',
  gear: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/>',
  power: '<path d="M12 3v8"/><path d="M6.3 6.8a8 8 0 1 0 11.4 0"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  play: '<rect x="2.5" y="5" width="19" height="14" rx="4"/><path d="m10 9 5 3-5 3z" fill="currentColor"/>',
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
  check: '<circle cx="12" cy="12" r="9"/><path d="m8 12 3 3 5-6"/>',
  alert: '<path d="M12 3 2 20h20z"/><path d="M12 10v4M12 17.5v.01"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7.5v.01"/>',
  x: '<path d="M6 6l12 12M18 6 6 18"/>',
};
function icon(name) {
  return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">${ICONS[name] || ""}</svg>`;
}
function hydrateIcons(root = document) {
  root.querySelectorAll("[data-icon]").forEach((el) => {
    if (!el.dataset.done) { el.innerHTML = icon(el.dataset.icon); el.dataset.done = "1"; }
  });
}

/* ===================================================================== bridge */
const TAURI = window.__TAURI__;
const invoke = TAURI ? TAURI.core.invoke : mockInvoke;
const listen = TAURI ? TAURI.event.listen : () => Promise.resolve();

/* ===================================================================== helpers */
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
const errText = (e) => (typeof e === "string" ? e : e?.message || JSON.stringify(e));

function toast(msg, kind = "ok", ms = 4500) {
  const el = document.createElement("div");
  el.className = `toast ${kind}`;
  el.innerHTML = `<span class="ico">${icon(kind === "err" ? "alert" : kind === "info" ? "info" : "check")}</span><div>${esc(msg)}</div>`;
  $("#toasts").appendChild(el);
  setTimeout(() => { el.classList.add("hide"); setTimeout(() => el.remove(), 300); }, ms);
}

function fmtUptime(sec) {
  const h = Math.floor(sec / 3600), m = Math.floor((sec % 3600) / 60), s = sec % 60;
  return h ? `${h} ч ${m} мин` : m ? `${m} мин ${s} с` : `${s} с`;
}
function plural(n, one, few, many) {
  const a = n % 10, b = n % 100;
  return a === 1 && b !== 11 ? one : a >= 2 && a <= 4 && (b < 10 || b >= 20) ? few : many;
}

/* ===================================================================== state */
let state = null;
let strategies = [];
let report = null;
let busy = false;
let testing = false;

async function refresh() {
  try {
    state = await invoke("get_state");
    testing = state.testing;
    render();
  } catch (e) {
    console.error(e);
  }
}

async function loadStrategies() {
  try { strategies = await invoke("get_strategies"); } catch (e) { toast(errText(e), "err"); }
  try { report = (await invoke("get_last_report")) || report; } catch {}
  renderStrategies();
}

/* ===================================================================== navigation */
function goto(page) {
  $$(".nav-item").forEach((b) => b.classList.toggle("active", b.dataset.page === page));
  $$(".page").forEach((p) => p.classList.toggle("active", p.id === `page-${page}`));
  if (page === "strategies") loadCommandLine();
  if (page === "lists") loadList(currentList);
  if (page === "logs") scrollLog();
}
$$(".nav-item").forEach((b) => b.addEventListener("click", () => goto(b.dataset.page)));
$$("[data-goto]").forEach((b) => b.addEventListener("click", () => goto(b.dataset.goto)));

/* ===================================================================== render */
function render() {
  if (!state) return;
  const st = state.status;
  const running = st.running;
  const s = state.settings;

  // sidebar
  const pill = $("#sidePill");
  pill.classList.toggle("on", running && !busy);
  pill.classList.toggle("busy", busy || testing);
  $("#sidePillText").textContent = testing ? "Подбор…" : busy ? "Подождите…" : running ? "Обход включён" : "Выключен";
  $("#versions").innerHTML = `Приложение ${esc(state.app_version)}<br>zapret2 ${esc(state.engine_version || "—")}`;

  // hero
  const power = $("#powerBtn");
  power.classList.toggle("on", running && !busy);
  power.classList.toggle("busy", busy || testing);
  $(".hero").classList.toggle("on", running && !busy);
  $("#heroState").textContent = testing ? "Идёт подбор стратегии" : busy ? (running ? "Перезапуск…" : "Запуск…") : running ? "Обход включён" : "Обход выключен";
  $("#heroSub").textContent = testing
    ? "Дождитесь окончания проверки на вкладке «Стратегии»"
    : running ? "Заблокированные сервисы должны открываться" : "Нажмите кнопку, чтобы включить";
  const strat = strategies.find((x) => x.id === s.strategy);
  $("#heroStrategyName").textContent = strat ? strat.name : s.strategy;
  updateUptime();

  // banners
  const banners = [];
  if (state.conflicts.length) {
    const names = [...new Set(state.conflicts.map((c) => c.name))].join(", ");
    banners.push(`<div class="banner warn"><span class="ico">${icon("alert")}</span><div>Запущены другие программы обхода (${esc(names)}). Они конфликтуют с Zapret2.</div><button class="btn small" id="killConflicts">Завершить</button></div>`);
  }
  if (st.last_error && !running) {
    banners.push(`<div class="banner error"><span class="ico">${icon("alert")}</span><div>${esc(st.last_error)}</div><button class="btn small ghost" data-goto="logs">Журнал</button></div>`);
  }
  $("#banners").innerHTML = banners.join("");
  $("#killConflicts")?.addEventListener("click", killConflicts);
  $$("#banners [data-goto]").forEach((b) => b.addEventListener("click", () => goto(b.dataset.goto)));

  // settings bindings
  $$("input[data-setting]").forEach((inp) => {
    const k = inp.dataset.setting;
    inp.checked = k === "autostart" ? !!state.autostart : !!s[k];
  });
  $$(".service").forEach((el) => el.classList.toggle("checked", !!s[el.dataset.key]));
  $$("#customMode button").forEach((b) => b.classList.toggle("active", b.dataset.mode === s.custom_mode));
  $("#customModeCard").hidden = !s.custom;
  const ud = state.user_domains, ad = state.auto_domains;
  $("#customDesc").textContent =
    s.custom_mode === "all" ? "Все сайты, кроме исключений"
    : s.custom_mode === "auto" ? `${ud} ${plural(ud, "сайт", "сайта", "сайтов")} + ${ad} найдено автоматически`
    : ud ? `${ud} ${plural(ud, "сайт", "сайта", "сайтов")} в списке` : "Список пуст — добавьте сайты";

  // settings page
  $("#engVer").textContent = state.engine_version || "—";
  $("#appVer").textContent = state.app_version;
  $("#dataDir").textContent = state.data_dir;
  if (document.activeElement !== $("#updateUrl")) $("#updateUrl").value = s.app_update_url || "";
  if (!s.app_update_url) {
    $("#appStatus").textContent = "Источник обновлений не настроен. Укажите адрес latest.json из ваших релизов на GitHub.";
  }

  $("#testBtn").disabled = testing;
  $("#cancelTestBtn").hidden = !testing;
  renderStrategySelection();
}

function updateUptime() {
  const st = state?.status;
  const el = $("#heroUptime");
  if (st?.running && st.since && !busy) {
    el.hidden = false;
    $("#heroUptimeText").textContent = fmtUptime(Math.max(0, Math.floor(Date.now() / 1000) - st.since));
  } else el.hidden = true;
}
setInterval(updateUptime, 1000);

/* ===================================================================== actions */
async function togglePower() {
  if (busy || testing || !state) return;
  busy = true; render();
  try {
    if (state.status.running) await invoke("stop_engine");
    else { await invoke("start_engine"); toast("Обход включён"); }
  } catch (e) {
    toast(errText(e), "err", 9000);
  } finally {
    busy = false;
    await refresh();
  }
}
$("#powerBtn").addEventListener("click", togglePower);

async function saveSettings(patch, { quiet = false } = {}) {
  const next = { ...state.settings, ...patch };
  const willRestart = state.status.running && ["strategy", "youtube", "discord", "discord_voice", "custom", "custom_mode", "quic", "debug_log", "custom_tls", "custom_http", "custom_quic"].some((k) => k in patch);
  if (willRestart) { busy = true; render(); }
  try {
    state.settings = await invoke("save_settings", { new: next });
    if (!quiet && willRestart) toast("Настройки применены, обход перезапущен");
  } catch (e) {
    toast(errText(e), "err", 9000);
  } finally {
    busy = false;
    await refresh();
  }
}

$$("input[data-setting]").forEach((inp) =>
  inp.addEventListener("change", () => saveSettings({ [inp.dataset.setting]: inp.checked }))
);
$$("#customMode button").forEach((b) => b.addEventListener("click", () => saveSettings({ custom_mode: b.dataset.mode })));

async function killConflicts() {
  try {
    const n = await invoke("kill_conflicts");
    toast(`Завершено процессов: ${n}`);
  } catch (e) { toast(errText(e), "err"); }
  refresh();
}

/* ===================================================================== strategies */
function resultFor(id) {
  return report?.results?.find((r) => r.id === id);
}
function badgeFor(r) {
  if (!r) return "";
  if (r.error) return `<span class="badge bad">ошибка</span>`;
  const cls = r.ok === r.total ? "good" : r.ok > 0 ? "mid" : "bad";
  return `<span class="badge ${cls}">${r.ok}/${r.total}${r.ok ? ` · ${r.avg_ms} мс` : ""}</span>`;
}

function renderStrategies() {
  const list = $("#strategyList");
  list.innerHTML = strategies.map((st) => {
    const r = resultFor(st.id);
    const best = report?.best === st.id ? `<span class="badge best">лучшая</span>` : "";
    const args = [...st.tls, ...st.http, ...(st.quic || [])].join(" ");
    const custom = st.id === "custom" ? `
      <div class="custom-fields" data-stop>
        <div><label>TLS (HTTPS)</label><textarea data-custom="custom_tls">${esc(state?.settings.custom_tls)}</textarea></div>
        <div><label>HTTP</label><textarea data-custom="custom_http">${esc(state?.settings.custom_http)}</textarea></div>
        <div><label>QUIC (пусто — как в остальных стратегиях)</label><textarea data-custom="custom_quic">${esc(state?.settings.custom_quic)}</textarea></div>
        <div><button class="btn small primary" data-save-custom>Сохранить параметры</button></div>
      </div>` : "";
    return `
      <div class="card strategy" data-id="${esc(st.id)}">
        <span class="radio"></span>
        <div>
          <div class="strategy-name">${esc(st.name)} ${best} ${badgeFor(r)}</div>
          <div class="strategy-desc">${esc(st.description)}</div>
          ${st.id === "custom" ? custom : `<button class="strategy-toggle" data-stop>Показать параметры</button><div class="strategy-args"><pre class="code">${esc(args)}</pre></div>`}
        </div>
        <div></div>
      </div>`;
  }).join("");

  $$(".strategy", list).forEach((el) => {
    el.addEventListener("click", (ev) => {
      if (ev.target.closest("[data-stop]")) return;
      if (state.settings.strategy !== el.dataset.id) saveSettings({ strategy: el.dataset.id }).then(loadCommandLine);
    });
  });
  $$(".strategy-toggle", list).forEach((b) => b.addEventListener("click", () => {
    const card = b.closest(".strategy");
    card.classList.toggle("open");
    b.textContent = card.classList.contains("open") ? "Скрыть параметры" : "Показать параметры";
  }));
  $("[data-save-custom]", list)?.addEventListener("click", async () => {
    const patch = {};
    $$("[data-custom]", list).forEach((t) => (patch[t.dataset.custom] = t.value.trim()));
    await saveSettings(patch, { quiet: true });
    toast("Параметры сохранены");
    loadStrategies();
    loadCommandLine();
  });
  renderStrategySelection();
}

function renderStrategySelection() {
  if (!state) return;
  $$(".strategy").forEach((el) => el.classList.toggle("selected", el.dataset.id === state.settings.strategy));
}

async function loadCommandLine() {
  try { $("#cmdLine").textContent = await invoke("get_command_line"); }
  catch (e) { $("#cmdLine").textContent = errText(e); }
}
$("#copyCmd").addEventListener("click", () => navigator.clipboard.writeText($("#cmdLine").textContent).then(() => toast("Скопировано")));

/* ---------- auto test */
let testRows = [];
function renderTestRows(runningIndex = -1) {
  $("#testRows").innerHTML = testRows.map((row, i) => {
    const r = row.result;
    const running = i === runningIndex && !r;
    const err = r?.error ? `<span class="err">${esc(r.error.split("\n")[0])}</span>` : "";
    return `<div class="test-row ${running ? "running" : ""}">
      <div>${esc(row.name)} ${err}</div>
      <div>${running ? `<span class="badge">проверка…</span>` : r ? badgeFor(r) : `<span class="badge">ожидание</span>`}</div>
      <div class="ms">${r && r.ok ? r.avg_ms + " мс" : ""}</div>
    </div>`;
  }).join("");
}

async function runTest() {
  if (testing) return;
  testing = true;
  testRows = [{ id: "baseline", name: "Без обхода", result: null }, ...strategies.map((s) => ({ id: s.id, name: s.name, result: null }))];
  $("#testPanel").hidden = false;
  $("#testFoot").hidden = true;
  $("#testTitle").textContent = "Подбор стратегии";
  $("#testSub").textContent = "Каждая стратегия запускается по очереди, и проверяется, открываются ли сайты. Это займёт около минуты.";
  $("#testBar").style.width = "0%";
  renderTestRows(0);
  render();
  try {
    report = await invoke("run_strategy_test");
    finishTest();
  } catch (e) {
    toast(errText(e), "err", 9000);
    $("#testSub").textContent = errText(e);
  } finally {
    testing = false;
    await refresh();
    renderStrategies();
  }
}

function finishTest() {
  $("#testBar").style.width = "100%";
  testRows.forEach((row) => {
    row.result = row.id === "baseline" ? report.baseline : report.results.find((r) => r.id === row.id) || row.result;
  });
  renderTestRows(-1);
  const base = report.baseline;
  const best = report.results.find((r) => r.id === report.best);
  $("#testTitle").textContent = report.cancelled ? "Подбор остановлен" : "Подбор завершён";
  let msg;
  if (base && base.total && base.ok === base.total) {
    msg = "Все адреса открываются и без обхода: похоже, блокировки сейчас нет.";
  } else if (best) {
    msg = `Лучший результат: «${best.name}» — ${best.ok} из ${best.total}, в среднем ${best.avg_ms} мс.`;
  } else {
    msg = "Ни одна стратегия не помогла. Попробуйте отключить QUIC, проверить антивирус или задать свои параметры.";
  }
  const foot = $("#testFoot");
  foot.hidden = false;
  foot.innerHTML = `<div>${esc(msg)}</div>${best && best.id !== state.settings.strategy ? `<button class="btn primary" id="applyBest">Применить</button>` : ""}`;
  $("#applyBest")?.addEventListener("click", async () => {
    await saveSettings({ strategy: best.id }, { quiet: true });
    toast(`Выбрана стратегия «${best.name}»`);
    $("#applyBest")?.remove();
    loadCommandLine();
  });
}

$("#testBtn").addEventListener("click", runTest);
$("#cancelTestBtn").addEventListener("click", () => invoke("cancel_strategy_test"));

listen("test-progress", ({ payload: p }) => {
  if (!testRows.length) return;
  if (p.result) {
    const row = testRows.find((r) => r.id === p.result.id) || (p.result.id === "baseline" ? testRows[0] : null);
    if (row) row.result = p.result;
  }
  $("#testCount").textContent = `${Math.min(p.index, p.total)} / ${p.total}`;
  $("#testBar").style.width = `${Math.round((Math.min(p.index, p.total) / p.total) * 100)}%`;
  renderTestRows(p.result ? -1 : p.index);
});

/* ===================================================================== lists */
const LIST_INFO = {
  user: { hint: "Сайты, к которым применяется обход, когда включён сервис «Мои сайты».", editable: true },
  exclude: { hint: "К этим сайтам обход не применяется никогда. Особенно важно в режиме «Все сайты».", editable: true },
  auto: { hint: "Сайты, которые нашлись автоматически в режиме «Список + авто»: после нескольких неудачных подключений домен добавляется сюда.", editable: true, clear: true },
  youtube: { hint: "Встроенный список, обновляется вместе с приложением. Свои сайты добавляйте в «Мои сайты».", editable: false },
  discord: { hint: "Встроенный список, обновляется вместе с приложением. Свои сайты добавляйте в «Мои сайты».", editable: false },
};
let currentList = "user";
let listDirty = false;

function countDomains(text) {
  return text.split(/\r?\n/).map((l) => l.trim()).filter((l) => l && !l.startsWith("#")).length;
}
function updateListCount() {
  const n = countDomains($("#listEditor").value);
  $("#listCount").textContent = `${n} ${plural(n, "домен", "домена", "доменов")}${listDirty ? " · есть несохранённые изменения" : ""}`;
}

async function loadList(name) {
  if (listDirty && !confirm("Есть несохранённые изменения. Отбросить их?")) return;
  currentList = name;
  listDirty = false;
  $$("#listTabs .tab").forEach((t) => t.classList.toggle("active", t.dataset.list === name));
  const info = LIST_INFO[name];
  $("#listHint").textContent = info.hint;
  $("#listEditor").readOnly = !info.editable;
  $("#listSave").hidden = !info.editable;
  $("#listClear").hidden = !info.clear;
  try { $("#listEditor").value = await invoke("read_list", { name }); }
  catch (e) { $("#listEditor").value = ""; toast(errText(e), "err"); }
  updateListCount();
}

$$("#listTabs .tab").forEach((t) => t.addEventListener("click", () => loadList(t.dataset.list)));
$("#listEditor").addEventListener("input", () => { listDirty = true; updateListCount(); });
$("#listSave").addEventListener("click", async () => {
  const btn = $("#listSave");
  btn.disabled = true;
  try {
    await invoke("write_list", { name: currentList, content: $("#listEditor").value });
    listDirty = false;
    toast(state?.status.running ? "Список сохранён, обход перезапущен" : "Список сохранён");
    updateListCount();
    refresh();
  } catch (e) { toast(errText(e), "err"); }
  finally { btn.disabled = false; }
});
$("#listClear").addEventListener("click", async () => {
  if (!confirm("Очистить автоматически найденные сайты?")) return;
  $("#listEditor").value = "";
  listDirty = true;
  $("#listSave").click();
});
$("#openListsDir").addEventListener("click", () => invoke("open_folder", { which: "lists" }));

/* ===================================================================== logs */
const MAX_LOG_LINES = 3000;
function logLine(text, withTime = true) {
  const log = $("#log");
  const div = document.createElement("div");
  const low = text.toLowerCase();
  if (text.startsWith("[app]")) div.className = "l-app";
  else if (low.includes("error") || low.includes("ошибк") || low.includes("fail")) div.className = "l-err";
  const t = withTime ? `<span class="t">${new Date().toLocaleTimeString("ru-RU")}</span> ` : "";
  div.innerHTML = t + esc(text);
  log.appendChild(div);
  while (log.childElementCount > MAX_LOG_LINES) log.firstElementChild.remove();
  if ($("#autoScroll").checked) scrollLog();
}
function scrollLog() { const l = $("#log"); l.scrollTop = l.scrollHeight; }
$("#copyLogs").addEventListener("click", () => navigator.clipboard.writeText($("#log").innerText).then(() => toast("Журнал скопирован")));
$("#clearLogs").addEventListener("click", async () => { await invoke("clear_logs"); $("#log").innerHTML = ""; });
listen("log", ({ payload }) => logLine(payload));

/* ===================================================================== updates */
function setStatus(el, text, cls = "") {
  el.textContent = text;
  el.className = `update-status ${cls}`;
}

$("#engCheck").addEventListener("click", async () => {
  const st = $("#engStatus");
  setStatus(st, "Проверяем…");
  try {
    const info = await invoke("check_engine_update");
    if (info.available) {
      setStatus(st, `Доступна версия ${info.latest} (опубликована ${new Date(info.published_at).toLocaleDateString("ru-RU")})`, "good");
      $("#engInstall").hidden = false;
    } else {
      setStatus(st, `Установлена последняя версия (${info.latest})`);
      $("#engInstall").hidden = true;
    }
  } catch (e) { setStatus(st, errText(e), "bad"); }
});
$("#engInstall").addEventListener("click", async () => {
  const btn = $("#engInstall");
  btn.disabled = true;
  try {
    const msg = await invoke("install_engine_update");
    setStatus($("#engStatus"), msg, "good");
    btn.hidden = true;
    toast(msg);
  } catch (e) { setStatus($("#engStatus"), errText(e), "bad"); }
  finally { btn.disabled = false; refresh(); }
});
listen("engine-update-progress", ({ payload }) => setStatus($("#engStatus"), payload));
listen("engine-updated", ({ payload }) => { toast(`zapret2 обновлён до ${payload}`); refresh(); });

$("#appCheck").addEventListener("click", async () => {
  const st = $("#appStatus");
  setStatus(st, "Проверяем…");
  try {
    const info = await invoke("check_app_update");
    if (!info.configured) setStatus(st, "Источник обновлений не настроен.", "bad");
    else if (info.available) { setStatus(st, `Доступна версия ${info.latest}${info.notes ? "\n" + info.notes : ""}`, "good"); $("#appInstall").hidden = false; }
    else setStatus(st, "Установлена последняя версия");
  } catch (e) { setStatus(st, errText(e), "bad"); }
});
$("#appInstall").addEventListener("click", async () => {
  $("#appInstall").disabled = true;
  setStatus($("#appStatus"), "Загрузка обновления…");
  try { await invoke("install_app_update"); }
  catch (e) { setStatus($("#appStatus"), errText(e), "bad"); $("#appInstall").disabled = false; }
});
listen("app-update-progress", ({ payload: [done, total] }) => {
  setStatus($("#appStatus"), total ? `Загрузка: ${Math.round((done / total) * 100)}%` : `Загружено ${Math.round(done / 1024)} КБ`);
});
listen("app-update-available", ({ payload }) => {
  toast(`Доступна новая версия приложения: ${payload.latest}. Установить можно в настройках.`, "info", 10000);
  setStatus($("#appStatus"), `Доступна версия ${payload.latest}`, "good");
  $("#appInstall").hidden = false;
});
$("#saveUpdateUrl").addEventListener("click", async () => {
  const v = $("#updateUrl").value.trim();
  if (v && !/^https:\/\//.test(v)) return toast("Адрес должен начинаться с https://", "err");
  await saveSettings({ app_update_url: v }, { quiet: true });
  toast("Источник обновлений сохранён");
});

$("#openDataDir").addEventListener("click", () => invoke("open_folder", { which: "root" }));
$$("[data-url]").forEach((a) => a.addEventListener("click", (e) => { e.preventDefault(); invoke("open_url", { url: a.dataset.url }); }));

/* ===================================================================== events */
listen("engine-state", () => { if (!busy) refresh(); });
listen("settings-changed", ({ payload }) => { if (state) { state.settings = payload; render(); } });
listen("toast", ({ payload }) => toast(payload, "err", 9000));

// Конфликтующие процессы и состояние автозапуска могут поменяться извне.
setInterval(() => { if (!document.hidden && !busy) refresh(); }, 5000);

/* ===================================================================== init */
(async function init() {
  hydrateIcons();
  await refresh();
  await loadStrategies();
  render();
  try { (await invoke("get_logs")).forEach((l) => logLine(l, false)); } catch {}
  if (state?.testing) { testing = true; render(); }
})();

/* ===================================================================== browser mock (только для предпросмотра без Tauri) */
function mockInvoke(cmd, args) {
  const m = (window.__mock ||= {
    running: false, since: null,
    settings: { strategy: "fake_multisplit_md5", youtube: true, discord: true, discord_voice: true, custom: false, custom_mode: "list", quic: true, autostart: false, start_minimized: true, engine_on_launch: true, close_to_tray: true, debug_log: false, engine_auto_update: true, app_auto_check: true, app_update_url: "", custom_tls: "--payload=tls_client_hello --lua-desync=multisplit:pos=1", custom_http: "", custom_quic: "" },
    lists: { user: "# Мои сайты\nrutracker.org\n", exclude: "gosuslugi.ru\n", auto: "", youtube: "youtube.com\ngooglevideo.com\n", discord: "discord.com\n" },
  });
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const strategies = [
    { id: "fake_multisplit_md5", name: "Фейк + разрезание (MD5)", description: "Универсальный вариант.", tls: ["--payload=tls_client_hello", "--lua-desync=fake:blob=tls_google:tcp_md5:repeats=6"], http: [], quic: null },
    { id: "seqovl", name: "Наложение последовательности (seqovl)", description: "Мягкий и быстрый способ.", tls: ["--lua-desync=multisplit:pos=1:seqovl=681"], http: [], quic: null },
    { id: "custom", name: "Свои параметры", description: "Параметры, заданные вручную.", tls: [], http: [], quic: null },
  ];
  switch (cmd) {
    case "get_state": return Promise.resolve({
      status: { running: m.running, since: m.since, last_error: null, command_line: "", pid: null },
      settings: m.settings, engine_version: "1.0.7", app_version: "1.0.0", conflicts: [], testing: false,
      data_dir: "C:\\Users\\user\\AppData\\Local\\Zapret2GUI", user_domains: 1, auto_domains: 0, autostart: m.settings.autostart,
    });
    case "start_engine": return wait(900).then(() => { m.running = true; m.since = Math.floor(Date.now() / 1000); });
    case "stop_engine": return wait(200).then(() => { m.running = false; });
    case "get_strategies": return Promise.resolve(strategies);
    case "save_settings": m.settings = args.new; return Promise.resolve(m.settings);
    case "get_command_line": return Promise.resolve("engine\\bin\\winws2.exe --wf-tcp-out=80,443 ...");
    case "read_list": return Promise.resolve(m.lists[args.name] || "");
    case "write_list": m.lists[args.name] = args.content; return Promise.resolve();
    case "get_logs": return Promise.resolve(["[app] Предпросмотр интерфейса без Tauri"]);
    case "run_strategy_test": return wait(1500).then(() => ({
      baseline: { id: "baseline", name: "Без обхода", ok: 1, total: 6, avg_ms: 300, checks: [] },
      results: [{ id: "fake_multisplit_md5", name: "Фейк + разрезание (MD5)", ok: 6, total: 6, avg_ms: 420, checks: [] }, { id: "seqovl", name: "seqovl", ok: 3, total: 6, avg_ms: 380, checks: [] }],
      best: "fake_multisplit_md5", cancelled: false,
    }));
    case "check_engine_update": return wait(500).then(() => ({ current: "1.0.7", latest: "1.0.7", available: false, published_at: "2026-11-05T08:01:55Z" }));
    case "check_app_update": return Promise.resolve({ configured: false, available: false });
    case "get_last_report": return Promise.resolve(null);
    default: return Promise.resolve(null);
  }
}
