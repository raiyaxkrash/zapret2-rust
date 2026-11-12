"""Обновляет список подсетей зарубежных хостингов и CDN (набор «Зарубежные хостинги»).

Берёт анонсируемые префиксы автономных систем из RIPEstat и пишет их, объединив
соседние подсети, в gui/resources/lists/packs/hosting.txt.
После обновления поднимите "version" в gui/resources/presets.json, чтобы файл
получили установленные копии.

    python gui/scripts/update-hosting-ipset.py
"""
import ipaddress
import json
import pathlib
import sys
import urllib.request

# Провайдер -> автономные системы.
PROVIDERS = {
    "Cloudflare": [13335],
    "Hetzner": [24940, 213230, 212317],
    "DigitalOcean": [14061],
    "OVHcloud": [16276],
    "Akamai/Linode": [63949],
    "Vultr": [20473],
    "Contabo": [51167],
    "Scaleway": [12876],
    "Amazon AWS": [16509, 14618],
    "Google Cloud": [396982],
    "Oracle Cloud": [31898],
    "Fastly": [54113],
    "Leaseweb": [60781, 28753],
}

URL = "https://stat.ripe.net/data/announced-prefixes/data.json?resource=AS{asn}&sourceapp=zapret2-gui"
OUT = pathlib.Path(__file__).resolve().parent.parent / "resources" / "lists" / "packs" / "hosting.txt"


def prefixes(asn: int) -> list:
    with urllib.request.urlopen(URL.format(asn=asn), timeout=60) as r:
        data = json.load(r)
    return [p["prefix"] for p in data["data"]["prefixes"]]


def main() -> int:
    v4, v6 = [], []
    lines = ["# Подсети зарубежных хостингов и CDN (RIPEstat, announced-prefixes).",
             "# Обновление: python gui/scripts/update-hosting-ipset.py"]
    for name, asns in PROVIDERS.items():
        count = 0
        for asn in asns:
            for p in prefixes(asn):
                net = ipaddress.ip_network(p, strict=False)
                (v4 if net.version == 4 else v6).append(net)
                count += 1
        lines.append(f"# {name}: AS{', AS'.join(map(str, asns))} ({count} префиксов)")
        print(f"{name}: {count}")
    nets = list(ipaddress.collapse_addresses(v4)) + list(ipaddress.collapse_addresses(v6))
    if len(nets) < 100:
        print("Слишком мало подсетей, файл не изменён", file=sys.stderr)
        return 1
    OUT.write_text("\n".join(lines + [str(n) for n in nets]) + "\n", encoding="utf-8", newline="\n")
    print(f"{OUT}: {len(nets)} подсетей")
    return 0


if __name__ == "__main__":
    sys.exit(main())
