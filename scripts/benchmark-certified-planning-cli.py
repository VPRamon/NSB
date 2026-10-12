#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Compare complete CLI processes: sampled compatibility, adaptive or certified."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time


def invoke(binary, arguments, memory_file):
    started = time.perf_counter()
    result = subprocess.run(
        ["/usr/bin/time", "-f", "%M", "-o", str(memory_file), str(binary), *arguments],
        capture_output=True, check=True, env={**os.environ, "RAYON_NUM_THREADS": "2"},
    )
    return (time.perf_counter() - started, int(memory_file.read_text().strip()),
            hashlib.sha256(result.stdout).hexdigest())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("previous", type=Path, help="baseline release CLI (same binary allowed with --adaptive)")
    parser.add_argument("current", type=Path)
    parser.add_argument("--repeats", type=int, default=20)
    parser.add_argument("--adaptive", action="store_true", help="measure practical adaptive schedules; compare determinism per variant, not byte equality between contracts")
    args = parser.parse_args()
    if args.repeats < 10:
        parser.error("at least ten repetitions are required for observed tail diagnostics")
    binaries = [args.previous.resolve(), args.current.resolve()]
    cases = []
    for site, profile in [("CTAO-S", "cta-south"), ("CTAO-N", "cta-north")]:
        for moon in ["jones2013", "ks1991"]:
            arguments = ["--format", "csv", "window", "--start", "2026-01-01T00:00:00Z",
                         "--end", "2027-01-01T00:00:00Z", "--site", site,
                         "--site-profile", profile, "--ra", "83.6331", "--dec", "22.0145",
                         "--max-nsb", "0.25", "--sun-altitude-max", "-18",
                         "--target-altitude-min", "20", "--components", "all",
                         "--moonlight-model", moon]
            cases.append((f"{site}/{moon}/all/year", "discrete compatibility", arguments, []))
    for moon in ["jones2013", "ks1991"]:
        arguments = ["--format", "csv", "window", "--start", "2026-01-14T01:00:00Z",
                     "--end", "2026-01-14T02:00:00Z", "--site", "PARANAL", "--ra", "83.6331",
                     "--dec", "22.0145", "--max-nsb", "0.25", "--components", "moon",
                     "--moonlight-model", moon, "--no-pre-filter"]
        cases.append((f"PARANAL/{moon}/moon-down/hour", "complete certified Moon-down",
                      arguments, ["--certified"]))
    with tempfile.TemporaryDirectory(prefix="nsb-235-cli-") as tmp:
        for label, contract, arguments, current_extra in cases:
            if args.adaptive:
                contract = "adaptive under resolved-model assumptions"
                current_extra = ["--adaptive"]
            memory_file = Path(tmp) / "rss.txt"
            previous_warmup = invoke(binaries[0], arguments, memory_file)
            current_warmup = invoke(binaries[1], arguments + current_extra, memory_file)
            if not args.adaptive:
                assert previous_warmup[2] == current_warmup[2], f"output differs: {label}"
            digests = [previous_warmup[2], current_warmup[2]]
            times, memory = [[], []], [[], []]
            for repeat in range(args.repeats):
                for variant in ([0, 1] if repeat % 2 == 0 else [1, 0]):
                    seconds, rss, digest = invoke(binaries[variant],
                        arguments + (current_extra if variant else []), memory_file)
                    assert digest == digests[variant], f"nondeterministic output: {label}"
                    times[variant].append(seconds)
                    memory[variant].append(rss)
            result = {"scenario": label, "contract": contract, "arguments": arguments,
                      "current_extra": current_extra, "repeats": args.repeats,
                      "rayon_threads": 2, "output_sha256": previous_warmup[2],
                      "current_output_sha256":current_warmup[2], "outputs_identical": digests[0] == digests[1],
                      "previous_binary_sha256": hashlib.sha256(binaries[0].read_bytes()).hexdigest(),
                      "current_binary_sha256": hashlib.sha256(binaries[1].read_bytes()).hexdigest()}
            for variant, name in enumerate(["previous", "current"]):
                ordered = sorted(times[variant])
                result.update({f"{name}_seconds": times[variant],
                    f"{name}_median_seconds": statistics.median(ordered),
                    f"{name}_observed_p90_seconds": ordered[math.ceil(0.9 * len(ordered)) - 1],
                    f"{name}_max_seconds": ordered[-1], f"{name}_rss_kib": memory[variant]})
            print(json.dumps(result, sort_keys=True), flush=True)


if __name__ == "__main__":
    main()
