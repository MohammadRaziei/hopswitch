#!/usr/bin/env python3
"""Runs the generated PAC files in a JavaScript engine and checks every answer.

    verify_local.py build/ [--engine node|quickjs|both] [--rounds 20]

Prints, per variant: size, load time, mean time per lookup, and how many sample hosts got a
different answer from the reference in samples.json. Exits 1 if any answer differs.
Node uses V8 (fast, JIT). QuickJS is a small interpreter: a rough stand-in for a weak PAC engine.
"""

import argparse
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

PRELUDE = "function isPlainHostName(h) { return h.indexOf('.') < 0; }\n"

RUNNER = """
function __run(hosts, rounds) {
  var out = [];
  for (var i = 0; i < hosts.length; i++) out.push(FindProxyForURL("http://" + hosts[i] + "/", hosts[i]));
  for (var r = 1; r < rounds; r++)
    for (var j = 0; j < hosts.length; j++) FindProxyForURL("http://" + hosts[j] + "/", hosts[j]);
  return out;
}
"""

NODE_SCRIPT = """
const fs = require('fs'), vm = require('vm');
const [pacPath, samplesPath, rounds] = process.argv.slice(2);
const pac = fs.readFileSync(pacPath, 'utf8');
const hosts = JSON.parse(fs.readFileSync(samplesPath, 'utf8')).hosts;
const ctx = vm.createContext({});
const t0 = process.hrtime.bigint();
vm.runInContext(%PRELUDE% + pac + %RUNNER%, ctx);
const t1 = process.hrtime.bigint();
ctx.__hosts = hosts;
const results = vm.runInContext('__run(__hosts, ' + Number(rounds) + ')', ctx);
const t2 = process.hrtime.bigint();
console.log(JSON.stringify({
  results: Array.from(results),
  load_ms: Number(t1 - t0) / 1e6,
  lookup_us: Number(t2 - t1) / 1e3 / (hosts.length * Number(rounds)),
}));
"""


def run_node(pac: Path, samples: Path, rounds: int) -> dict:
    script = NODE_SCRIPT.replace("%PRELUDE%", json.dumps(PRELUDE)).replace("%RUNNER%", json.dumps(RUNNER))
    with tempfile.NamedTemporaryFile("w", suffix=".js", delete=False) as f:
        f.write(script)
    out = subprocess.run(["node", f.name, str(pac), str(samples), str(rounds)], capture_output=True, text=True, check=True)
    return json.loads(out.stdout)


def run_quickjs(pac: Path, samples: Path, rounds: int) -> dict:
    import quickjs

    hosts = json.loads(samples.read_text())["hosts"]
    ctx = quickjs.Context()
    t0 = time.perf_counter()
    ctx.eval(PRELUDE + pac.read_text() + RUNNER)
    t1 = time.perf_counter()
    results = json.loads(str(ctx.eval(f"JSON.stringify(__run({json.dumps(hosts)}, {rounds}))")))
    t2 = time.perf_counter()
    return {"results": results, "load_ms": (t1 - t0) * 1e3, "lookup_us": (t2 - t1) * 1e6 / (len(hosts) * rounds)}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("build", type=Path)
    parser.add_argument("--engine", choices=["node", "quickjs", "both"], default="both")
    parser.add_argument("--rounds", type=int, default=20)
    args = parser.parse_args()

    samples = args.build / "samples.json"
    data = json.loads(samples.read_text())
    engines = {"node": run_node, "quickjs": run_quickjs}
    chosen = engines if args.engine == "both" else {args.engine: engines[args.engine]}

    bad = 0
    print(f"{'engine':<9}{'variant':<13}{'size':>11}{'load ms':>10}{'lookup us':>11}{'wrong':>7}")
    for engine, run in chosen.items():
        for variant, want in data["expect"].items():
            pac = args.build / f"{variant}.pac"
            got = run(pac, samples, args.rounds)
            wrong = [(h, w, g) for h, w, g in zip(data["hosts"], want, got["results"]) if w != g]
            bad += len(wrong)
            print(f"{engine:<9}{variant:<13}{pac.stat().st_size:>11,}{got['load_ms']:>10.1f}{got['lookup_us']:>11.1f}{len(wrong):>7}")
            for host, w, g in wrong[:5]:
                print(f"    {host}: expected {w}, got {g}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
