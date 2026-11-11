"use strict";
/* Мини-окно в трее. */

let st = null;
let strategies = [];
let busy = false;

async function refresh() {
  try {
    st = await invoke("get_state");
    applyAppearance(st.settings.theme, st.platform);
    if (!strategies.length) strategies = await invoke("get_strategies");
    render();
  } catch (e) { console.error(e); }
}

function render() {
  if (!st) return;
  const running = st.status.running;
  const s = st.settings;
  const strat = strategies.find((x) => x.id === s.strategy);

  const p = $("#power");
  p.classList.toggle("on", running && !busy);
  p.classList.toggle("busy", busy || st.testing);
  $("#state").textContent = st.testing ? "Подбор стратегии…" : busy ? (running ? "Выключение…" : "Запуск…") : running ? "Обход включён" : "Обход выключен";
  $("#sub").textContent = running && st.status.since
    ? `Работает ${fmtUptime(Math.max(0, Math.floor(Date.now() / 1000) - st.status.since))}`
    : "Нажмите, чтобы включить";

  const sel = $("#strategy");
  if (sel.options.length !== strategies.length) {
    sel.innerHTML = strategies.map((x) => `<option value="${esc(x.id)}">${esc(x.name)}</option>`).join("");
  }
  sel.value = s.strategy;
  sel.disabled = st.testing;

  $$(".tile").forEach((t) => t.classList.toggle("on", !!s[t.dataset.key]));

  const warn = $("#warn");
  if (st.conflicts.length) {
    warn.className = "fly-warn";
    warn.textContent = "Запущены другие программы обхода. Откройте главное окно, чтобы их завершить.";
    warn.hidden = false;
  } else if (st.status.last_error && !running) {
    warn.className = "fly-warn err";
    warn.textContent = st.status.last_error;
    warn.hidden = false;
  } else warn.hidden = true;
}

async function save(patch) {
  const running = st.status.running;
  st.settings = { ...st.settings, ...patch };
  if (running) busy = true;
  render();
  try { st.settings = await invoke("save_settings", { new: st.settings }); }
  catch (e) { showError(e); }
  finally { busy = false; await refresh(); }
}

function showError(e) {
  const warn = $("#warn");
  warn.className = "fly-warn err";
  warn.textContent = errText(e);
  warn.hidden = false;
}

$("#power").addEventListener("click", async () => {
  if (busy || !st || st.testing) return;
  busy = true; render();
  try {
    if (st.status.running) await invoke("stop_engine");
    else await invoke("start_engine");
  } catch (e) { showError(e); }
  finally { busy = false; await refresh(); }
});

$("#strategy").addEventListener("change", (e) => save({ strategy: e.target.value }));
$$(".tile").forEach((t) => t.addEventListener("click", () => save({ [t.dataset.key]: !st.settings[t.dataset.key] })));

$("#openMain").addEventListener("click", () => invoke("open_main", { page: "home" }));
$("#settings").addEventListener("click", () => invoke("open_main", { page: "settings" }));
$("#test").addEventListener("click", async () => {
  await invoke("open_main", { page: "strategies" });
  TAURI?.event.emitTo("main", "start-test");
});
$("#quit").addEventListener("click", () => invoke("quit_app"));
document.addEventListener("keydown", (e) => { if (e.key === "Escape") invoke("hide_flyout"); });

listen("flyout-shown", refresh);
listen("engine-state", () => { if (!busy) refresh(); });
listen("settings-changed", ({ payload }) => { if (st) { st.settings = payload; applyAppearance(payload.theme); render(); } });
listen("content-updated", async () => { strategies = await invoke("get_strategies"); render(); });
setInterval(() => { if (!document.hidden) render(); }, 1000);

hydrateIcons();
refresh();
