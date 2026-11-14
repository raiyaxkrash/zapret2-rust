"""Статистика Zapret2 GUI без телеметрии: только счётчики скачиваний GitHub.

* Скачивания установщика по версиям — сколько раз скачан *-setup.exe каждого релиза.
* Активные установки (оценка) — приложение проверяет обновления, скачивая latest.json
  последнего релиза при запуске и раз в 6 часов. Прирост этого счётчика за сутки,
  делённый на 4, примерно равен числу копий, работавших в эти сутки.

Скрипт дописывает строку в history.csv и пересобирает README.md в каталоге OUT.
    python stats.py <каталог>   (переменная GITHUB_TOKEN — по желанию)
"""
import csv
import datetime as dt
import json
import os
import pathlib
import sys
import urllib.request

REPO = "raiyaxkrash/zapret2-rust"
CHECKS_PER_DAY = 4


def api(path):
    req = urllib.request.Request(f"https://api.github.com/repos/{REPO}/{path}",
                                 headers={"Accept": "application/vnd.github+json", "User-Agent": "zapret2-gui-stats"})
    token = os.environ.get("GITHUB_TOKEN")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.load(r)


def main(out: pathlib.Path) -> None:
    out.mkdir(parents=True, exist_ok=True)
    releases = api("releases?per_page=100")
    today = dt.date.today().isoformat()

    per_version = []
    latest_json_total = 0
    for rel in releases:
        if rel.get("draft"):
            continue
        setup = sum(a["download_count"] for a in rel["assets"] if a["name"].endswith("-setup.exe"))
        checks = sum(a["download_count"] for a in rel["assets"] if a["name"] == "latest.json")
        latest_json_total += checks
        per_version.append((rel["tag_name"], rel.get("published_at", "")[:10], setup, checks))

    history = out / "history.csv"
    rows = []
    if history.exists():
        with history.open(encoding="utf-8") as f:
            rows = [r for r in csv.DictReader(f) if r["date"] != today]
    installers_total = sum(v[2] for v in per_version)
    rows.append({"date": today, "installers": installers_total, "update_checks": latest_json_total})
    with history.open("w", encoding="utf-8", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["date", "installers", "update_checks"])
        w.writeheader()
        w.writerows(rows)

    # Оценка активных установок по приросту проверок обновлений между соседними днями.
    active = []
    for prev, cur in zip(rows, rows[1:]):
        days = (dt.date.fromisoformat(cur["date"]) - dt.date.fromisoformat(prev["date"])).days or 1
        delta = int(cur["update_checks"]) - int(prev["update_checks"])
        active.append((cur["date"], max(0, round(delta / days / CHECKS_PER_DAY))))

    lines = [
        "# Статистика Zapret2 GUI",
        "",
        f"Обновлено: {today}. Источник — только счётчики скачиваний GitHub, программа ничего не отправляет.",
        "",
        f"- **Скачиваний установщика всего:** {installers_total}",
        f"- **Активных установок (оценка за последние сутки):** {active[-1][1] if active else 'появится завтра'}",
        "",
        "Активные установки оцениваются по проверкам обновлений: каждая работающая копия скачивает",
        f"`latest.json` примерно {CHECKS_PER_DAY} раза в сутки. Это приблизительная цифра.",
        "",
        "## Скачивания по версиям",
        "",
        "| Версия | Дата | Установщик | Проверок обновлений, пока версия была последней |",
        "| --- | --- | ---: | ---: |",
    ]
    lines += [f"| {t} | {d} | {s} | {c} |" for t, d, s, c in per_version]
    if active:
        lines += ["", "## Активные установки по дням (оценка)", "", "| Дата | Установок |", "| --- | ---: |"]
        lines += [f"| {d} | {n} |" for d, n in reversed(active[-30:])]
    (out / "README.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"installers={installers_total} update_checks={latest_json_total}")


if __name__ == "__main__":
    main(pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "stats-out"))
