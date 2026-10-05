#!/usr/bin/env python3
"""Extract diagnostics for the check-flow harness; never rewrite the raw log."""

import json
import re
import sys


ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
# Tool-specific additions to the Rust matcher: rumdl, typos, taplo, repository
# annotations, and current Rust panic formatting. Unknown formats stay in logs.
OTHER_DIAGNOSTICS = re.compile(
    r"^(?:warning|warn|error)(?:\[[^]]+\])?:"
    r"|^\s*(?:WARN|WARNING|ERROR)\b"
    r"|^.+:\d+(?::\d+)?:\s*(?:warning|error|[A-Z]{2}\d{3})\b"
    r"|^::(?:warning|error)(?:\s[^:]*)?::"
    r"|^thread .+ panicked at "
    r"|^\S+ required$"
)
CONTEXT = re.compile(r"^\s*(?:-->|:::|\d+\s*\||\||= (?:help|note):)")
SUMMARY = re.compile(r'^warning: .+ generated \d+ warnings?(?: \(.*\))?$')


def emit_diagnostics(matcher_path, log_path):
    with open(matcher_path, encoding="utf-8") as source:
        matchers = json.load(source)["problemMatcher"]
    # Each matcher starts with a diagnostic header. Multiline location/context
    # lines are emitted after that header, including an incomplete diagnostic.
    headers = [re.compile(matcher["pattern"][0]["regexp"]) for matcher in matchers]
    locations = [
        re.compile(pattern["regexp"])
        for matcher in matchers
        for pattern in matcher["pattern"][1:]
    ]
    in_diagnostic = False
    with open(log_path, encoding="utf-8", errors="replace") as source:
        for raw in source:
            line = ANSI.sub("", raw.rstrip("\r\n"))
            if SUMMARY.match(line):
                in_diagnostic = False
                continue
            if any(pattern.match(line) for pattern in headers) or OTHER_DIAGNOSTICS.search(line):
                sys.stderr.write(raw if raw.endswith("\n") else raw + "\n")
                in_diagnostic = True
            elif in_diagnostic and (
                any(pattern.match(line) for pattern in locations) or CONTEXT.match(line)
            ):
                sys.stderr.write(raw if raw.endswith("\n") else raw + "\n")
            elif line:
                in_diagnostic = False


if __name__ == "__main__":
    emit_diagnostics(*sys.argv[1:])
