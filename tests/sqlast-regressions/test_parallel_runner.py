#!/usr/bin/env python3
"""Fast queue/report contracts, maintained by workspace regression verification."""
import copy
import json
from pathlib import Path
import tempfile
import time
import unittest
from unittest.mock import Mock, patch
import xml.etree.ElementTree as ET

import parallel_runner as subject
import run_once
import scheduling


class ReportContracts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name)
        self.group = dict(id="pilot-17", names=["pilot::one", "pilot::two"])
        self.workers = [dict(worker=0, exit=0, completions=[dict(id="pilot-17", status=0)])]
        (self.path / "worker-0").mkdir()
        self.xml = self.path / "worker-0/pilot-17.xml"
        self.xml.write_text('<testsuites><testsuite tests="2" failures="0" errors="0"><testcase name="pilot::one" time="1"/><testcase name="pilot::two" time="2"/></testsuite></testsuites>')

    def result(self):
        subject.write_json(self.path / "worker-0/completion.json", self.workers[0])
        return subject.aggregate([self.group], self.workers, self.path)

    def test_success(self):
        result = self.result()
        self.assertEqual(result["problems"], [])
        suite = ET.parse(self.path / "combined.xml").find("testsuite")
        self.assertEqual(suite.get("name"), "pilot-17")
        self.assertEqual(suite.find("testcase").get("time"), "1")

    def test_outcome_details_are_preserved(self):
        for tag, counter in [("failure", "failures"), ("error", "errors"), ("skipped", "skipped")]:
            with self.subTest(tag=tag):
                self.xml.write_text(f'<testsuites><testsuite tests="2" failures="{int(tag == "failure")}" errors="{int(tag == "error")}" skipped="{int(tag == "skipped")}"><testcase name="pilot::one" time="1"><{tag} message="detail">evidence</{tag}></testcase><testcase name="pilot::two"/></testsuite></testsuites>')
                result = self.result()
                self.assertEqual(result["outcomes"][counter], 1)
                self.assertEqual(bool(result["problems"]), tag != "skipped")
                element = ET.parse(self.path / "combined.xml").find(f"testsuite/testcase/{tag}")
                self.assertEqual(element.text, "evidence")
                self.assertEqual(element.get("message"), "detail")

    def test_worker_failure_with_passing_xml(self):
        self.workers[0]["exit"] = 97
        self.assertTrue(self.result()["problems"])

    def test_missing_worker_exit_or_completion(self):
        self.workers[0]["exit"] = None
        self.assertTrue(self.result()["problems"])
        self.workers[0]["exit"] = 0
        self.workers[0]["completions"] = []
        self.assertTrue(self.result()["problems"])

    def test_missing_and_malformed_xml(self):
        self.xml.unlink()
        self.assertTrue(self.result()["problems"])
        self.xml.write_text('<testsuites><testsuite')
        self.assertTrue(self.result()["problems"])

    def test_duplicate_replacing_missing_case(self):
        self.xml.write_text(self.xml.read_text().replace("pilot::two", "pilot::one"))
        self.assertTrue(self.result()["problems"])

    def test_duplicate_and_unexpected_groups(self):
        original = copy.deepcopy(self.workers)
        self.workers[0]["completions"] *= 2
        self.assertTrue(self.result()["problems"])
        self.workers = original
        self.workers[0]["completions"][0]["id"] = "unknown-18"
        self.assertTrue(self.result()["problems"])

    def test_missing_completion_record(self):
        self.assertTrue(subject.aggregate([self.group], self.workers, self.path)["problems"])

    def test_unexpected_report(self):
        (self.path / "worker-0/unexpected.xml").write_text(self.xml.read_text())
        self.assertTrue(self.result()["problems"])

    def test_declared_counter_mismatch(self):
        self.xml.write_text(self.xml.read_text().replace('tests="2"', 'tests="3"'))
        self.assertTrue(self.result()["problems"])


class ServerIdentityContracts(unittest.TestCase):
    def test_server_death_and_registration_replacement(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp)
            server = subject.Server({}, path, path)
            server.process = Mock()
            server.process.poll.return_value = None
            server.registration = b"owned registration"
            registration = path / "sql-fun-server.pid"
            registration.write_bytes(server.registration)
            server.verify()
            registration.write_bytes(b"replacement registration")
            with self.assertRaises(RuntimeError):
                server.verify()
            registration.write_bytes(server.registration)
            server.process.poll.return_value = 1
            with self.assertRaises(RuntimeError):
                server.verify()


class QueueContracts(unittest.TestCase):
    def test_priority_order_and_name_ties(self):
        groups = [dict(id=ident) for ident in ("short", "long-b", "long-a")]
        self.assertEqual([g["id"] for g in subject.ordered_tasks(groups, {"short": 1, "long-a": 3, "long-b": 3})], ["long-a", "long-b", "short"])
        with self.assertRaises(ValueError):
            subject.ordered_tasks(groups, {"short": 1})

    def run_queue(self, count, fail=False):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp)
            # Real persistent shell processes exercise the coordinator protocol.
            (path / "worker.sh").write_text('''#!/usr/bin/env bash
set -eu
while IFS=$'\\t' read -r fixture level schema count version; do
  if [[ "$fixture" == fail ]]; then exit 93; fi
  sleep 0.01
  printf '{"fixture":"%s","version":"%s","status":0,"seconds":0.01}\\n' "$fixture" "$version"
done
''')
            groups = [dict(id=f"g{i}-17", fixture=("fail" if fail and i == 0 else f"g{i}"), level="builtin", schema="-", count=1, version="17") for i in range(12)]
            class Server:
                def verify(self):
                    pass
            with patch.object(subject, "SUITE", path):
                return groups, subject.run_workers(groups, count, {}, path, path, Server())

    def test_concurrent_claims_exactly_once_and_idle_workers(self):
        for count in (1, 4, 16):
            with self.subTest(workers=count):
                groups, workers = self.run_queue(count)
                self.assertTrue(all(w["exit"] == 0 for w in workers))
                claims = [g for w in workers for g in w["claims"]]
                completions = [c["id"] for w in workers for c in w["completions"]]
                self.assertCountEqual(claims, [g["id"] for g in groups])
                self.assertCountEqual(completions, claims)

    def test_server_failure_interrupts_waiting_workers(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp)
            (path / "worker.sh").write_text("read -r task; sleep 60\n")
            began = time.monotonic()
            class Server:
                def verify(self):
                    if time.monotonic() - began > 0.1:
                        raise RuntimeError("simulated owned server death")
            group = dict(id="g-17", fixture="g", level="builtin", schema="-", count=1, version="17")
            with patch.object(subject, "SUITE", path):
                workers = subject.run_workers([group], 1, {}, path, path, Server())
            self.assertLess(time.monotonic() - began, 3)
            self.assertIn("simulated owned server death", workers[0]["problem"])

    def test_death_after_claim_is_not_requeued_or_hidden(self):
        groups, workers = self.run_queue(4, fail=True)
        claims = [g for w in workers for g in w["claims"]]
        self.assertEqual(claims.count(groups[0]["id"]), 1)
        self.assertTrue(any(w.get("problem") or w["exit"] != 0 for w in workers))
        self.assertNotIn(groups[0]["id"], [c["id"] for w in workers for c in w["completions"]])


class PriorityContracts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name)
        self.groups = [dict(id=f"group-{n:03}") for n in range(140)]
        self.durations = {group["id"]: float(n) for n, group in enumerate(self.groups)}
        self.timings = self.path / "timings.tsv"
        self.output = self.path / "priorities.tsv"

    def generate(self, durations=None):
        data = self.durations if durations is None else durations
        self.timings.write_text("".join(f"{name}\t{value}\n" for name, value in data.items()))
        scheduling.generate(self.timings, self.output, self.groups)

    def test_tiers_cover_inventory_and_sort_names(self):
        self.generate()
        priorities = scheduling.read_priorities(self.output, self.groups)
        self.assertEqual([sum(value == tier for value in priorities.values()) for tier in (3, 2, 1)], [10, 65, 65])
        self.assertEqual([name for name, value in priorities.items() if value == 3], [f"group-{n:03}" for n in range(130, 140)])
        self.assertNotIn("139.0", self.output.read_text())

    def test_intra_tier_timing_changes_do_not_change_file(self):
        self.generate()
        previous = self.output.read_bytes()
        modified = dict(self.durations)
        modified['group-138'], modified['group-139'] = modified['group-139'], modified['group-138']
        self.generate(modified)
        self.assertEqual(self.output.read_bytes(), previous)
        modified['group-129'], modified['group-130'] = modified['group-130'], modified['group-129']
        self.generate(modified)
        self.assertNotEqual(self.output.read_bytes(), previous)

    def test_tied_timings_and_odd_or_small_inventories(self):
        for count in (1, 9, 10, 11, 13, 139, 140, 141):
            groups = self.groups[:count] if count <= 140 else self.groups + [dict(id='group-140')]
            rows = scheduling.classify(groups[::-1], {g['id']: 1 for g in groups})
            self.assertEqual([name for _, name in rows], sorted(g['id'] for g in groups))
            self.assertEqual(len(rows), count)
            heavy, medium, light = scheduling.tier_sizes(count)
            self.assertEqual(heavy, min(10, count))
            self.assertIn(medium - light, (0, 1))

    def test_incomplete_or_invalid_timings_preserve_existing_file(self):
        self.generate()
        previous = self.output.read_bytes()
        for data in ({}, {**self.durations, 'unknown': 1}, {**self.durations, 'group-000': float('nan')}, {**self.durations, 'group-000': -1}):
            with self.subTest(data_size=len(data)), self.assertRaises(ValueError):
                self.generate(data)
            self.assertEqual(self.output.read_bytes(), previous)
        self.timings.write_text('group-000\t1\ngroup-000\t2\n')
        with self.assertRaises(ValueError):
            scheduling.generate(self.timings, self.output, self.groups)

    def test_missing_duplicate_unknown_and_misordered_priorities_fail(self):
        self.generate()
        valid = self.output.read_text()
        variants = [valid.replace('heavy\tgroup-130\n', ''),
                    valid.replace('group-130', 'group-131'),
                    valid.replace('group-130', 'unknown'),
                    valid.replace('heavy\tgroup-130', 'invalid\tgroup-130'),
                    valid.replace('heavy\tgroup-130', 'medium\tgroup-130'),
                    valid.replace('heavy\tgroup-130\nheavy\tgroup-131', 'heavy\tgroup-131\nheavy\tgroup-130')]
        for content in variants:
            with self.subTest(content=content[:60]), self.assertRaises(ValueError):
                self.output.write_text(content)
                scheduling.read_priorities(self.output, self.groups)

    def test_ci_runner_executes_single_suite(self):
        self.generate()
        reports = self.path / 'reports'
        with patch.object(subject, 'inventory', return_value=self.groups[::-1]), \
             patch.object(subject, 'validate_environment'), \
             patch.object(subject, 'record_environment'), \
             patch.object(subject, 'run_suite', return_value={'ok': True}) as run_suite:
            args = ['--priorities', str(self.output), '--output', str(reports)]
            self.assertEqual(run_once.main(args), 0)
            run_suite.assert_called_once()
            groups, workers, weights, _, _ = run_suite.call_args.args
            self.assertEqual(workers, 2)
            self.assertEqual([g['id'] for g in groups], sorted(g['id'] for g in self.groups))
            self.assertEqual(set(weights.values()), {1, 2, 3})
            run_suite.reset_mock(return_value=True)
            run_suite.return_value = {'ok': False}
            self.assertEqual(run_once.main(args), 1)
            run_suite.assert_called_once()
            self.output.unlink()
            run_suite.reset_mock()
            with self.assertRaises(FileNotFoundError):
                run_once.main(args)
            run_suite.assert_not_called()

    def test_ci_preflight_only_validates_committed_file(self):
        self.generate()
        previous = self.output.read_bytes()
        with patch.object(subject, 'inventory', return_value=self.groups), \
             patch.object(scheduling, 'generate') as generate:
            scheduling.main(['--check', str(self.output)])
            generate.assert_not_called()
        self.assertEqual(self.output.read_bytes(), previous)


if __name__ == "__main__":
    unittest.main()
