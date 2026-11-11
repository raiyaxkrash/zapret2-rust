"use strict";
/* Главное окно. Общие функции — в common.js. */

let state = null;
let strategies = [];
let report = null;
let busy = false;
let testing = false;

async function refresh() {
  try {
    state = await invoke("get_state");
    testing = state.testing;
    applyAppearance(state.settings.theme, state.platform, state.settings.animations);
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

/* ===================================================================== window controls */
if (appWindow) {
  $("#winMin").addEventListener("click", () => appWindow.minimize());
  $("#winMax").addEventListener("click", () => appWindow.toggleMaximize());
  $("#winClose").addEventListener("click", () => appWindow.close());
  const syncMax = async () => {
    const max = await appWindow.isMaximized();
    $("#winMax span").innerHTML = iconSvg(max ? "restore" : "max");
    $("#winMax").title = max ? "Восстановить" : "Развернуть";
  };
  appWindow.onResized(syncMax);
  appWindow.onFocusChanged(({ payload }) => $(".titlebar").classList.toggle("inactive", !payload));
  syncMax();
} else {
  $(".caption").hidden = true;
}

/* ===================================================================== navigation */
function goto(page) {
  $$(".nav-item").forEach((b) => b.classList.toggle("active", b.dataset.page === page));
  $$(".page").forEach((p) => p.classList.toggle("active", p.id === `page-${page}`));
  $(".content").scrollTop = 0;
  if (page === "strategies") loadCommandLine();
  if (page === "lists") loadList(currentList);
  if (page === "logs") scrollLog();
}
$$(".nav-item").forEach((b) => b.addEventListener("click", () => goto(b.dataset.page)));
$$("[data-goto]").forEach((b) => b.addEventListener("click", () => goto(b.dataset.goto)));
listen("navigate", ({ payload }) => goto(payload));

/* ===================================================================== render */
function render() {
  if (!state) return;
  const st = state.status;
  const running = st.running;
  const s = state.settings;
  const strat = strategies.find((x) => x.id === s.strategy);

  // статус в навигации
  const ns = $("#navStatus");
  ns.classList.toggle("on", running && !busy);
  ns.classList.toggle("busy", busy || testing);
  $("#navStatusTitle").textContent = testing ? "Подбор стратегии…" : busy ? "Подождите…" : running ? "Обход включён" : "Обход выключен";
  $("#navStatusSub").textContent = `zapret2 ${state.engine_version || "—"} · v${state.app_version}`;

  // главный блок
  const power = $("#powerBtn");
  power.classList.toggle("on", running && !busy);
  power.classList.toggle("busy", busy || testing);
  $(".hero").classList.toggle("on", running && !busy);
  $("#heroState").textContent = testing ? "Идёт подбор стратегии" : busy ? (running ? "Перезапуск…" : "Запуск…") : running ? "Обход включён" : "Обход выключен";
  $("#heroSub").textContent = testing
    ? "Дождитесь окончания проверки на странице «Стратегии»"
    : running ? "Заблокированные сервисы должны открываться" : "Нажмите кнопку, чтобы включить";
  $("#heroStrategyName").textContent = strat ? strat.name : s.strategy;
  const ytStrat = s.youtube_strategy ? strategies.find((x) => x.id === s.youtube_strategy) : null;
  $("#heroYt").hidden = !ytStrat || !s.youtube;
  $("#heroYtName").textContent = ytStrat ? `YouTube: ${ytStrat.name}` : "";
  updateUptime();

  const ytSel = $("#ytStrategy");
  const ytOpts = `<option value="">Как основная</option>` + strategies.map((x) => `<option value="${esc(x.id)}">${esc(x.name)}</option>`).join("");
  if (ytSel.dataset.opts !== ytOpts) { ytSel.innerHTML = ytOpts; ytSel.dataset.opts = ytOpts; }
  ytSel.value = s.youtube_strategy || "";

  // уведомления
  const banners = [];
  if (state.conflicts.length) {
    const names = [...new Set(state.conflicts.map((c) => c.name))].join(", ");
    banners.push(`<div class="infobar warn">${icon("alert")}<div>Запущены другие программы обхода (${esc(names)}). Они конфликтуют с Zapret2.</div><button class="btn" id="killConflicts">Завершить</button></div>`);
  }
  if (st.last_error && !running) {
    banners.push(`<div class="infobar error">${icon("alert")}<div>${esc(st.last_error)}</div><button class="btn" data-goto="logs">Журнал</button></div>`);
  }
  $("#banners").innerHTML = banners.join("");
  $("#killConflicts")?.addEventListener("click", killConflicts);
  $$("#banners [data-goto]").forEach((b) => b.addEventListener("click", () => goto(b.dataset.goto)));

  // переключатели
  $$("input[data-setting]").forEach((inp) => {
    const k = inp.dataset.setting;
    inp.checked = k === "autostart" ? !!state.autostart : !!s[k];
    const lbl = inp.closest(".setting-control")?.querySelector(".state-label");
    if (lbl) lbl.textContent = inp.checked ? "Вкл." : "Откл.";
  });
  $$(".services .setting").forEach((el) => el.classList.toggle("checked", !!s[el.dataset.key]));
  $$("#customMode button").forEach((b) => b.classList.toggle("active", b.dataset.mode === s.custom_mode));
  $$("#themeSel button").forEach((b) => b.classList.toggle("active", b.dataset.theme === (s.theme || "system")));
  $("#customModeCard").hidden = !s.custom;
  const ud = state.user_domains, ad = state.auto_domains;
  $("#customDesc").textContent =
    s.custom_mode === "all" ? "Все сайты, кроме исключений"
    : s.custom_mode === "auto" ? `${ud} ${plural(ud, "сайт", "сайта", "сайтов")} + ${ad} найдено автоматически`
    : ud ? `${ud} ${plural(ud, "сайт", "сайта", "сайтов")} в списке` : "Список пуст — добавьте сайты";

  // настройки
  $("#engVer").textContent = state.engine_version || "—";
  $("#appVer").textContent = state.app_version;
  $("#contentVer").textContent = state.content_version || "—";
  $("#dataDir").textContent = state.data_dir;
  if (document.activeElement !== $("#updateUrl")) $("#updateUrl").value = s.app_update_url || "";

  renderUpdatePill(state.app_update);
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
everyWhileVisible(1000, updateUptime);

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

const ENGINE_KEYS = ["strategy", "youtube_strategy","youtube", "discord", "discord_voice", "custom", "custom_mode", "quic", "debug_log", "custom_tls", "custom_http", "custom_quic"];

async function saveSettings(patch, { quiet = false } = {}) {
  const next = { ...state.settings, ...patch };
  const willRestart = state.status.running && ENGINE_KEYS.some((k) => k in patch);
  if (willRestart) { busy = true; render(); }
  if ("theme" in patch) applyAppearance(patch.theme);
  if ("animations" in patch) applyAppearance(null, null, patch.animations);
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
$$("#themeSel button").forEach((b) => b.addEventListener("click", () => saveSettings({ theme: b.dataset.theme })));
$("#ytStrategy").addEventListener("change", (e) => saveSettings({ youtube_strategy: e.target.value }).then(loadCommandLine));

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
function scoreBadge(label, ok, total, ms) {
  const cls = ok === total ? "good" : ok > 0 ? "mid" : "bad";
  return `<span class="badge ${cls}">${label}${ok}/${total}${ms && ok ? ` · ${ms} мс` : ""}</span>`;
}
function badgeFor(r, { split = true } = {}) {
  if (!r) return "";
  if (r.error) return `<span class="badge bad">ошибка</span>`;
  if (split && r.yt_total && r.other_total) {
    return scoreBadge("YouTube ", r.yt_ok, r.yt_total) + " " + scoreBadge("Остальные ", r.other_ok, r.other_total);
  }
  return scoreBadge("", r.ok, r.total, r.avg_ms);
}

function renderStrategies() {
  const list = $("#strategyList");
  list.innerHTML = strategies.map((st) => {
    const r = resultFor(st.id);
    const best = (report?.best === st.id ? `<span class="badge best">лучшая</span>` : "")
      + (report?.best_youtube === st.id ? ` <span class="badge best">лучшая для YouTube</span>` : "")
      + (state?.settings.youtube_strategy === st.id ? ` <span class="badge">выбрана для YouTube</span>` : "");
    const args = [...st.tls, ...st.http, ...(st.quic || [])].join(" ");
    const extra = st.id === "custom" ? `
      <div class="custom-fields" data-stop>
        <div><label>TLS (HTTPS)</label><textarea data-custom="custom_tls">${esc(state?.settings.custom_tls)}</textarea></div>
        <div><label>HTTP</label><textarea data-custom="custom_http">${esc(state?.settings.custom_http)}</textarea></div>
        <div><label>QUIC (пусто — как в остальных стратегиях)</label><textarea data-custom="custom_quic">${esc(state?.settings.custom_quic)}</textarea></div>
        <div><button class="btn primary" data-save-custom>Сохранить параметры</button></div>
      </div>`
      : `<button class="link-btn" data-stop data-toggle-args>Показать параметры</button><div class="strategy-args"><pre class="code">${esc(args)}</pre></div>`;
    return `
      <div class="card strategy" data-id="${esc(st.id)}">
        <span class="radio"></span>
        <div>
          <div class="strategy-name">${esc(st.name)} ${best} ${badgeFor(r)}</div>
          <div class="strategy-desc">${esc(st.description)}</div>
          ${extra}
        </div>
      </div>`;
  }).join("");

  $$(".strategy", list).forEach((el) => {
    el.addEventListener("click", (ev) => {
      if (ev.target.closest("[data-stop]")) return;
      if (state.settings.strategy !== el.dataset.id) saveSettings({ strategy: el.dataset.id }).then(loadCommandLine);
    });
  });
  $$("[data-toggle-args]", list).forEach((b) => b.addEventListener("click", () => {
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
  if (state) render();
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

/* ---------- автоподбор */
let testRows = [];
function renderTestRows(runningIndex = -1) {
  $("#testRows").innerHTML = testRows.map((row, i) => {
    const r = row.result;
    const running = i === runningIndex && !r;
    const err = r?.error ? `<span class="err">${esc(r.error.split("\n")[0])}</span>` : "";
    return `<div class="test-row ${running ? "running" : ""}">
      <div>${esc(row.name)}${err}</div>
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
  const bestYt = report.results.find((r) => r.id === report.best_youtube);
  $("#testTitle").textContent = report.cancelled ? "Подбор остановлен" : "Подбор завершён";

  const lines = [];
  if (base && base.total && base.ok === base.total) {
    lines.push("Все адреса открываются и без обхода: похоже, блокировки сейчас нет.");
  } else {
    if (best) {
      const [ok, total] = base.yt_total && base.other_total ? [best.other_ok, best.other_total] : [best.ok, best.total];
      lines.push(`${bestYt ? "Discord и остальные" : "Лучшая"}: «${best.name}» — ${ok} из ${total}.`);
    }
    if (bestYt) lines.push(`YouTube: «${bestYt.name}» — ${bestYt.yt_ok} из ${bestYt.yt_total}.`);
    else if (base.yt_total) lines.push("Для YouTube ни одна стратегия не сработала. Попробуйте выключить QUIC и повторить подбор.");
    if (!best && !bestYt) lines.push("Ни одна стратегия не помогла. Попробуйте выключить QUIC, проверить антивирус или задать свои параметры.");
  }

  // Что применить: основную и отдельную для YouTube (если отличается).
  const patch = {};
  if (best && best.id !== state.settings.strategy) patch.strategy = best.id;
  if (bestYt) {
    const main = patch.strategy || state.settings.strategy;
    const yt = bestYt.id === main ? "" : bestYt.id;
    if (yt !== (state.settings.youtube_strategy || "")) patch.youtube_strategy = yt;
  }
  const foot = $("#testFoot");
  foot.hidden = false;
  foot.innerHTML = `<div>${lines.map(esc).join("<br>")}</div>${Object.keys(patch).length ? `<button class="btn primary" id="applyBest">Применить</button>` : ""}`;
  $("#applyBest")?.addEventListener("click", async () => {
    await saveSettings(patch, { quiet: true });
    toast("Стратегии применены");
    $("#applyBest")?.remove();
    renderStrategies();
    loadCommandLine();
  });
}

$("#testBtn").addEventListener("click", runTest);
$("#cancelTestBtn").addEventListener("click", () => invoke("cancel_strategy_test"));
listen("start-test", () => { goto("strategies"); runTest(); });

listen("test-progress", ({ payload: p }) => {
  if (!testRows.length) return;
  if (p.result) {
    const row = testRows.find((r) => r.id === p.result.id);
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
  auto: { hint: "Сайты, найденные автоматически в режиме «Список + авто»: после нескольких неудачных подключений домен добавляется сюда.", editable: true, clear: true },
  youtube: { hint: "Встроенный список, обновляется автоматически с GitHub. Свои сайты добавляйте в «Мои сайты».", editable: false },
  discord: { hint: "Встроенный список, обновляется автоматически с GitHub. Свои сайты добавляйте в «Мои сайты».", editable: false },
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
  if (listDirty && name !== currentList && !confirm("Есть несохранённые изменения. Отбросить их?")) return;
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

$("#contentCheck").addEventListener("click", async () => {
  const st = $("#contentStatus");
  const btn = $("#contentCheck");
  btn.disabled = true;
  setStatus(st, "Проверяем…");
  try {
    const msg = await invoke("update_content");
    setStatus(st, msg, "good");
    await loadStrategies();
    refresh();
  } catch (e) { setStatus(st, errText(e), "bad"); }
  finally { btn.disabled = false; }
});
listen("content-updated", ({ payload }) => {
  toast(`Стратегии и списки обновлены (версия ${payload})`);
  loadStrategies();
  refresh();
});

$("#engCheck").addEventListener("click", async () => {
  const st = $("#engStatus");
  setStatus(st, "Проверяем…");
  try {
    const info = await invoke("check_engine_update");
    if (info.available) {
      setStatus(st, `Доступна версия ${info.latest} от ${new Date(info.published_at).toLocaleDateString("ru-RU")}`, "good");
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
  toast(`Доступна версия ${payload.latest}. Нажмите «Обновить» вверху окна.`, "info", 8000);
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
listen("settings-changed", ({ payload }) => {
  if (!state) return;
  const strategyChanged = state.settings.strategy !== payload.strategy;
  state.settings = payload;
  applyAppearance(payload.theme, null, payload.animations);
  render();
  if (strategyChanged) loadCommandLine();
});
listen("toast", ({ payload }) => toast(payload, "err", 9000));

// Конфликтующие процессы и состояние автозапуска могут поменяться извне.
everyWhileVisible(5000, () => { if (!busy) refresh(); });
onVisible(() => { if (!busy) refresh(); });

/* ===================================================================== init */
(async function init() {
  hydrateIcons();
  await refresh();
  await loadStrategies();
  try { (await invoke("get_logs")).forEach((l) => logLine(l, false)); } catch {}
  if (state?.testing) { testing = true; render(); }
})();
