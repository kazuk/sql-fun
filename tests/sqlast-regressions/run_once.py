#!/usr/bin/env python3
"""Internal CI runner: one shared-server suite using committed priority tiers."""
import argparse
import os
from pathlib import Path
import signal
import sys
import tempfile

import parallel_runner
import scheduling


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--priorities", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    groups = sorted(parallel_runner.inventory(), key=lambda group: group["id"])
    priorities = scheduling.read_priorities(args.priorities, groups)
    parallel_runner.validate_environment(parser, groups)
    args.output.mkdir(parents=True, exist_ok=True)
    # Each invocation retains fresh evidence even when the report root is reused.
    reports = Path(tempfile.mkdtemp(prefix="run-", dir=args.output))
    parallel_runner.record_environment(reports, groups)
    (reports / "priorities.tsv").write_text(args.priorities.read_text())
    # Values are tier preferences, not duration estimates. Stable ties use names.
    result = parallel_runner.run_suite(groups, 2, priorities, reports / "workers-2", os.environ)
    print(f"Regression evidence: {reports}", flush=True)
    for problem in result.get("problems", []):
        print(f"Regression failure: {problem}", file=sys.stderr)
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, parallel_runner.interrupted)
    raise SystemExit(main())
