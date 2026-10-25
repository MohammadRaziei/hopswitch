#!/usr/bin/env python3
"""Spike for ADR-0011: how big is a PAC script for the real domain lists, and does it still work?

Builds several PAC variants from the two public lists and writes, next to them:
    samples.json   hosts with the expected answer, for checking an engine
    stats.json     sizes and counts (also printed)

Variants (all answer DIRECT for listed domains and PROXY for the rest):
    object-full   one object key per domain, the whole Iran list        (simplest, biggest)
    object-ir     same, but the 69k `.ir` entries become one `*.ir` rule
    bucket-ir     `*.ir` rule + domains grouped by TLD and first two letters, "label,label,"
                  strings searched with indexOf                          (smallest)
    fod           the FOD list (developer services that need a proxy): PROXY for listed, DIRECT otherwise

The generated JavaScript is ES3 (no let/const, no arrow functions, no trailing commas) because
Windows' PAC engine is old. This is a throw-away spike: the real generator will live in Rust.

usage: make_pac.py --iran iran.txt --fod fod.txt --out build/
"""

import argparse
import gzip
import json
import random
import re
import sys
from collections import defaultdict
from pathlib import Path

LABEL = r"[a-z0-9](?:[a-z0-9-]*[a-z0-9])?"
DOMAIN = re.compile(rf"^{LABEL}(?:\.{LABEL})+$")
PROXY_ANSWER = "PROXY 127.0.0.1:9"  # a placeholder: the benchmark never connects through it


def load(path: Path) -> tuple[list[str], int]:
    """Normalised, de-duplicated domains and the number of lines that were dropped."""
    seen, dropped = set(), 0
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip().lower().lstrip(".")
        if not line or line.startswith("#"):
            continue
        if DOMAIN.match(line):
            seen.add(line)
        else:
            dropped += 1
    return sorted(seen), dropped


def js(value) -> str:
    return json.dumps(value, separators=(",", ":"), ensure_ascii=True)


PLAIN_AND_IR = """  if (isPlainHostName(host)) return "DIRECT";
  host = host.toLowerCase();
  var n = host.length;
  if (n > 3 && host.substring(n - 3) == ".ir") return "DIRECT";
"""


def object_pac(domains: list[str], ir_rule: bool, hit: str, miss: str) -> str:
    table = ",".join(f'"{d}":1' for d in domains)
    head = PLAIN_AND_IR if ir_rule else '  if (isPlainHostName(host)) return "DIRECT";\n  host = host.toLowerCase();\n'
    return f"""var D = {{{table}}};
function FindProxyForURL(url, host) {{
{head}  var parts = host.split(".");
  for (var i = 0; i < parts.length - 1; i++) {{
    if (D[parts.slice(i).join(".")] === 1) return "{hit}";
  }}
  return "{miss}";
}}
"""


def bucket_pac(domains: list[str], hit: str, miss: str) -> str:
    """TLD -> first two letters of the rest -> ",rest,rest,". `.ir` is a rule, not data."""
    tlds: dict[str, dict[str, list[str]]] = defaultdict(lambda: defaultdict(list))
    for d in domains:
        rest, _, tld = d.rpartition(".")
        tlds[tld][rest[:2]].append(rest)
    table = {
        tld: {key: "," + ",".join(rests) + "," for key, rests in sorted(buckets.items())}
        for tld, buckets in sorted(tlds.items())
    }
    return f"""var T = {js(table)};
var has = Object.prototype.hasOwnProperty;
function FindProxyForURL(url, host) {{
{PLAIN_AND_IR}  var parts = host.split(".");
  var tld = parts[parts.length - 1];
  if (!has.call(T, tld)) return "{miss}";
  var buckets = T[tld];
  for (var i = 0; i < parts.length - 1; i++) {{
    var rest = parts.slice(i, parts.length - 1).join(".");
    var bucket = buckets[rest.substr(0, 2)];
    if (bucket && bucket.indexOf("," + rest + ",") >= 0) return "{hit}";
  }}
  return "{miss}";
}}
"""


def expected(host: str, listed: set[str], ir_rule: bool, hit: str, miss: str) -> str:
    """The reference answer, written independently of the generated JavaScript."""
    host = host.lower()
    if "." not in host:
        return "DIRECT"
    if ir_rule and host.endswith(".ir"):
        return hit
    parts = host.split(".")
    if any(".".join(parts[i:]) in listed for i in range(len(parts) - 1)):
        return hit
    return miss


def sample_hosts(iran: list[str], seed: int = 7) -> list[str]:
    rnd = random.Random(seed)
    non_ir = [d for d in iran if not d.endswith(".ir")]
    ir = [d for d in iran if d.endswith(".ir")]
    hosts = ["digikala.com", "aparat.com", "www.digikala.com", "a.b.c.digikala.com"]
    hosts += rnd.sample(non_ir, 120) + rnd.sample(ir, 40)
    hosts += ["www." + d for d in rnd.sample(non_ir, 60)]
    hosts += ["x-" + d for d in rnd.sample(non_ir, 40)]  # look-alikes: must NOT match their base
    hosts += [d[1:] for d in rnd.sample(non_ir, 40)]  # drop a letter: also look-alikes
    hosts += [f"{rnd.choice('abcdefghijklmnopqrstuvwxyz')}{i}.not-listed-{i}.com" for i in range(120)]
    hosts += [f"unlisted-{i}.ir" for i in range(40)]  # .ir but not in the list
    hosts += ["example.com", "google.com", "github.com", "constructor.com", "hasOwnProperty.net"]
    hosts += ["localhost", "intranet", "ir", "com", "DIGIKALA.COM", "WwW.ApArAt.CoM"]
    return list(dict.fromkeys(hosts))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--iran", type=Path, required=True)
    parser.add_argument("--fod", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    iran, iran_dropped = load(args.iran)
    fod, fod_dropped = load(args.fod)
    non_ir = [d for d in iran if not d.endswith(".ir")]
    listed_full, listed_nonir = set(iran), set(non_ir)

    variants = {
        "object-full": (object_pac(iran, False, "DIRECT", PROXY_ANSWER), listed_full, False),
        "object-ir": (object_pac(non_ir, True, "DIRECT", PROXY_ANSWER), listed_nonir, True),
        "bucket-ir": (bucket_pac(non_ir, "DIRECT", PROXY_ANSWER), listed_nonir, True),
        "fod": (object_pac(fod, False, PROXY_ANSWER, "DIRECT"), set(fod), False),
    }

    stats = {
        "iran": {"domains": len(iran), "dropped": iran_dropped, "dot_ir": len(iran) - len(non_ir), "non_ir": len(non_ir)},
        "fod": {"domains": len(fod), "dropped": fod_dropped},
        "variants": {},
    }
    hosts = sample_hosts(iran) + random.Random(3).sample(fod, min(60, len(fod)))
    hosts = list(dict.fromkeys(hosts))
    samples = {"hosts": hosts, "expect": {}}

    for name, (source, listed, ir_rule) in variants.items():
        (args.out / f"{name}.pac").write_text(source, encoding="ascii")
        raw = source.encode("ascii")
        stats["variants"][name] = {"bytes": len(raw), "gzip_bytes": len(gzip.compress(raw, 9))}
        hit, miss = ("PROXY 127.0.0.1:9", "DIRECT") if name == "fod" else ("DIRECT", "PROXY 127.0.0.1:9")
        samples["expect"][name] = [expected(h, listed, ir_rule, hit, miss) for h in hosts]

    (args.out / "samples.json").write_text(json.dumps(samples), encoding="utf-8")
    (args.out / "stats.json").write_text(json.dumps(stats, indent=2), encoding="utf-8")

    print(f"iran list: {len(iran):,} domains ({stats['iran']['dot_ir']:,} are .ir, {len(non_ir):,} are not); {iran_dropped} lines dropped")
    print(f"fod list : {len(fod):,} domains; {fod_dropped} lines dropped")
    print(f"{'variant':<12}{'bytes':>12}{'gzip':>12}")
    for name, v in stats["variants"].items():
        print(f"{name:<12}{v['bytes']:>12,}{v['gzip_bytes']:>12,}")
    print(f"{len(hosts)} sample hosts written")
    return 0


if __name__ == "__main__":
    sys.exit(main())
