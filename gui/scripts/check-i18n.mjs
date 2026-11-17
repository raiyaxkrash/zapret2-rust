#!/usr/bin/env node
// Проверка полноты английского перевода (ui/i18n.js) для HTML, JS и presets.json.
// Запуск: node scripts/check-i18n.mjs

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const guiRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const i18nSrc = fs.readFileSync(path.join(guiRoot, "ui", "i18n.js"), "utf8");

const CYR = /[А-Яа-яЁё]/;
const reEsc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

// Извлекаем пары ["ru", "en"] из массива D в ui/i18n.js
const pairRe = /\[\s*"((?:[^"\\]|\\.)*)"\s*,\s*"((?:[^"\\]|\\.)*)"\s*\]/g;
const exact = new Map();
const patterns = [];
let totalPairs = 0;

for (const m of i18nSrc.matchAll(pairRe)) {
  const ru = JSON.parse(`"${m[1]}"`);
  const en = JSON.parse(`"${m[2]}"`);
  totalPairs++;
  if (!ru.includes("{}")) {
    exact.set(ru, en);
  } else {
    const parts = ru.split("{}");
    const re = new RegExp("^" + parts.map(reEsc).join("([\\s\\S]+?)") + "$");
    patterns.push({ re, weight: parts.join("").length });
  }
}
patterns.sort((a, b) => b.weight - a.weight);

function isTranslated(raw, depth = 0) {
  const s = raw.trim();
  if (!s || !CYR.test(s)) return true;
  if (exact.has(s)) return true;
  const flat = s.replace(/\s+/g, " ");
  if (exact.has(flat)) return true;
  if (depth > 3) return false;
  for (const p of patterns) {
    const m = p.re.exec(s) || p.re.exec(flat);
    if (m) return true;
  }
  if (depth === 0) {
    const sentences = flat.split(/(?<=[.!?:])\s+(?=[А-ЯЁA-Z])/);
    if (sentences.length > 1 && sentences.every((part) => isTranslated(part, 1))) {
      return true;
    }
  }
  return false;
}

/** Лексер строковых и шаблонных литералов JS с поддержкой вложенных `${...}`. */
function extractJsStrings(code) {
  const results = [];
  let i = 0;
  const n = code.length;

  function readQuoted(q) {
    let s = "";
    i++; // пропускаем открывающую кавычку
    while (i < n && code[i] !== q) {
      if (code[i] === "\\") {
        const next = code[i + 1];
        if (next === "n") s += "\n";
        else if (next === "t") s += "\t";
        else s += next ?? "";
        i += 2;
      } else {
        s += code[i++];
      }
    }
    i++; // закрывающая кавычка
    return s;
  }

  function readTemplate() {
    let s = "";
    i++; // пропускаем `
    while (i < n && code[i] !== "`") {
      if (code[i] === "\\") {
        const next = code[i + 1];
        s += next === "n" ? "\n" : (next ?? "");
        i += 2;
      } else if (code[i] === "$" && code[i + 1] === "{") {
        i += 2;
        s += "{}";
        scanCode(true);
      } else {
        s += code[i++];
      }
    }
    i++; // закрывающая `
    return s;
  }

  function scanCode(stopAtBrace) {
    let depth = 1;
    while (i < n) {
      const ch = code[i];
      if (ch === "/" && code[i + 1] === "/") {
        while (i < n && code[i] !== "\n") i++;
      } else if (ch === "/" && code[i + 1] === "*") {
        i += 2;
        while (i < n && !(code[i] === "*" && code[i + 1] === "/")) i++;
        i += 2;
      } else if (ch === '"' || ch === "'") {
        const start = i;
        let val = readQuoted(ch);
        // Склеиваем "a" + "b"
        while (i < n) {
          let j = i;
          while (j < n && /\s/.test(code[j])) j++;
          if (code[j] === "+") {
            j++;
            while (j < n && /\s/.test(code[j])) j++;
            if (code[j] === '"' || code[j] === "'") {
              i = j;
              val += readQuoted(code[j]);
              continue;
            }
          }
          break;
        }
        results.push({ val, start });
      } else if (ch === "`") {
        const start = i;
        const val = readTemplate();
        results.push({ val, start });
      } else if (stopAtBrace && ch === "{") {
        depth++;
        i++;
      } else if (stopAtBrace && ch === "}") {
        depth--;
        i++;
        if (depth === 0) return;
      } else {
        i++;
      }
    }
  }

  scanCode(false);
  return results;
}

const missing = [];

// 1. HTML-файлы (текстовые узлы и атрибуты title/placeholder/aria-label, кроме data-no-i18n)
for (const file of ["index.html", "tray.html"]) {
  const html = fs
    .readFileSync(path.join(guiRoot, "ui", file), "utf8")
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/<(script|style|textarea|pre)\b[\s\S]*?<\/\1>/gi, "");

  for (const m of html.matchAll(/<([a-z0-9-]+)([^>]*)>/gi)) {
    const attrs = m[2];
    if (/\bdata-no-i18n\b/i.test(attrs)) continue;
    for (const am of attrs.matchAll(/\b(?:title|placeholder|aria-label)="([^"]+)"/gi)) {
      if (!isTranslated(am[1])) missing.push(`[ui/${file} attr] ${am[1]}`);
    }
  }

  const cleanHtml = html.replace(/<([a-z0-9-]+)\b[^>]*\bdata-no-i18n\b[^>]*>[\s\S]*?<\/\1>/gi, "");
  for (const tm of cleanHtml.matchAll(/>([^<]+)</g)) {
    const txt = tm[1].replace(/\s+/g, " ").trim();
    if (txt && !isTranslated(txt)) missing.push(`[ui/${file} text] ${txt}`);
  }
}

// 2. presets.json (названия и описания стратегий, наборов и провайдеров)
const presets = JSON.parse(fs.readFileSync(path.join(guiRoot, "resources", "presets.json"), "utf8"));
for (const group of [presets.isp || [], presets.packs || [], presets.warp_packs || [], presets.strategies || []]) {
  for (const item of group) {
    for (const key of ["name", "description"]) {
      if (item[key] && !isTranslated(item[key])) {
        missing.push(`[presets.json ${item.id || item.name}.${key}] ${item[key]}`);
      }
    }
  }
}

// 3. Строки в ui/app.js и ui/tray.js
const JS_IGNORE = new Set([
  "ошибк",
  "не удал",
  "с обходом",
  "без обхода",
  " остановлена",
  " завершена",
  " мс",
  "мс",
  "«{}»",
  " · YouTube: ",
  "Discord и остальные",
  "Лучшая",
  "Проверка {}{}",
  "{}: «{}» — {} из {}.",
  "Для ARM64 нет подписанного драйвера WinDivert, поэтому Windows загрузит его только в режиме тестовой подписи. {}",
  "У вас установлен {}. Zapret2 отключит его (вернуть можно в Настройках){}.",
  "и перенесёт ваши сайты: {}",
  "Сначала выключите Secure Boot в настройках UEFI/BIOS, затем включите тестовый режим здесь.",
  "Включите тестовый режим и перезагрузите компьютер.",
  "· обход выключен",
  "· через обход",
  "· браузеры и программы",
  "· есть несохранённые изменения",
]);

for (const file of ["app.js", "tray.js"]) {
  const src = fs.readFileSync(path.join(guiRoot, "ui", file), "utf8");
  for (const { val } of extractJsStrings(src)) {
    if (!CYR.test(val) || JS_IGNORE.has(val)) continue;

    for (const am of val.matchAll(/\b(?:title|placeholder|aria-label)="([^"]+)"/gi)) {
      const attrVal = am[1].trim();
      if (CYR.test(attrVal) && !isTranslated(attrVal)) {
        missing.push(`[ui/${file} attr] ${attrVal}`);
      }
    }

    for (const chunk of val.split(/<\/?[a-z][^>]*>/gi)) {
      const norm = chunk.trim();
      if (!norm || JS_IGNORE.has(norm) || !CYR.test(norm)) continue;
      const strippedLead = norm.replace(/^(?:\{\}\s*)+/, "").replace(/(?:\s*\{\})+$/, "");
      if (
        !isTranslated(norm) &&
        !JS_IGNORE.has(strippedLead) &&
        !(strippedLead !== norm && isTranslated(strippedLead))
      ) {
        missing.push(`[ui/${file}] ${norm.replace(/\n/g, "\\n")}`);
      }
    }
  }
}

if (missing.length > 0) {
  console.error(`Найдены строки без перевода в ui/i18n.js (${missing.length}):`);
  for (const line of [...new Set(missing)]) console.error("  -", line);
  process.exit(1);
}

console.log(`OK: проверено ${totalPairs} переводов в ui/i18n.js — пропущенных строк нет.`);
