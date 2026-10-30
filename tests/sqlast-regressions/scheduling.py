#!/usr/bin/env python3
"""Internal generation and validation of committed regression priority tiers."""
import argparse
import math
from pathlib import Path
import tempfile

import parallel_runner

TIERS = ("heavy", "medium", "light")


def tier_sizes(count):
    heavy = min(10, count)
    remaining = count - heavy
    return heavy, (remaining + 1) // 2, remaining // 2


def classify(groups, durations):
    expected = {group["id"] for group in groups}
    if not expected or len(expected) != len(groups) or set(durations) != expected:
        raise ValueError("Timings must cover the exact nonempty group inventory")
    if any(not math.isfinite(value) or value < 0 for value in durations.values()):
        raise ValueError("Invalid group duration")
    ranked = sorted(expected, key=lambda name: (-durations[name], name))
    result, offset = [], 0
    for tier, size in zip(TIERS, tier_sizes(len(groups))):
        result.extend((tier, name) for name in sorted(ranked[offset:offset + size]))
        offset += size
    return result


def read_priorities(path, groups):
    rows = []
    for line in path.read_text().splitlines():
        if line and not line.startswith("#"):
            tier, name = line.split("\t")
            if tier not in TIERS:
                raise ValueError(f"Unknown priority tier: {tier}")
            rows.append((tier, name))
    names = [name for _, name in rows]
    expected = {group["id"] for group in groups}
    if len(names) != len(expected) or set(names) != expected:
        raise ValueError("Priorities contain missing, duplicate or unknown groups")
    if tuple(sum(tier == value for tier, _ in rows) for value in TIERS) != tier_sizes(len(expected)):
        raise ValueError("Priority tier sizes do not match the inventory")
    if rows != sorted(rows, key=lambda row: (TIERS.index(row[0]), row[1])):
        raise ValueError("Priorities must be tier-ordered and name-sorted")
    return {name: len(TIERS) - TIERS.index(tier) for tier, name in rows}


def generate(timings, output, groups):
    durations = {}
    for line in timings.read_text().splitlines():
        name, seconds = line.split("\t")
        if name in durations:
            raise ValueError(f"Duplicate group timing: {name}")
        durations[name] = float(seconds)
    rows = classify(groups, durations)
    content = "# priority\tgroup\n" + "".join(f"{tier}\t{name}\n" for tier, name in rows)
    if not output.exists() or output.read_text() != content:
        # Validate the complete input before replacing the tracked file.
        with tempfile.TemporaryDirectory(dir=output.parent) as temporary:
            path = Path(temporary) / output.name
            path.write_text(content)
            path.replace(output)
    print(f"Regression priorities: {dict(zip(TIERS, tier_sizes(len(groups))))}; {output}")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--timings", type=Path)
    mode.add_argument("--check", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    if args.check:
        if args.output:
            parser.error("--check does not generate an output file")
        read_priorities(args.check, parallel_runner.inventory())
        print(f"Regression priorities validated: {args.check}")
    else:
        if not args.output:
            parser.error("--timings requires --output")
        generate(args.timings, args.output, parallel_runner.inventory())


if __name__ == "__main__":
    main()
