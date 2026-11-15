"""Mode-specific perf preflight and native report extraction."""
import hashlib
import os
import shlex
import signal
import subprocess


EVENTS = ("task-clock", "context-switches", "page-faults")


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def run(command, output, **kwargs):
    print(f"+ {shlex.join(map(str, command))}", flush=True)
    process = subprocess.Popen(list(map(str, command)), stdout=output,
                               start_new_session=True, **kwargs)
    try:
        status = process.wait()
        if status:
            raise subprocess.CalledProcessError(status, command)
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()


class Tools:
    def __init__(self, args, metadata):
        self.args, self.metadata = args, metadata
        self.env = dict(os.environ, LC_ALL="C")
        self.events = []
        self.metadata["profiling"] = dict(event="cpu-clock:u", frequency_hz=99, call_graph="fp") \
            if args.mode == "record" else dict(events=self.events)

    def command(self, directory, workload):
        if self.args.mode == "record":
            return ["perf", "record", "-e", "cpu-clock:u", "-F", "99", "--call-graph", "fp",
                    "-o", str(directory / "perf.data"), "--", *map(str, workload)]
        return ["perf", "stat", "-e", ",".join(self.events),
                "-o", str(directory / "stat.txt"), "--", *map(str, workload)]

    def check(self, reports):
        self.metadata["tools"] = dict(perf=subprocess.check_output(
            ["perf", "version"], env=self.env, text=True).strip())
        if self.args.mode == "record":
            self.metadata["tools"].update(
                flamegraph_directory=str(self.args.flamegraph),
                flamegraph_revision=subprocess.check_output(
                    ["git", "-C", str(self.args.flamegraph), "rev-parse", "HEAD"], text=True).strip(),
                flamegraph_sha256={name: digest(self.args.flamegraph / name)
                                   for name in ("stackcollapse-perf.pl", "flamegraph.pl")},
                demangler=subprocess.check_output(["c++filt", "--version"], text=True).splitlines()[0])
            readable = subprocess.check_output(["c++filt", "--format=rust"],
                                               input="_RNvC4test4main\n", text=True)
            if "test" not in readable or "::main" not in readable:
                raise RuntimeError("c++filt must support Rust v0 symbols")
        smoke = reports / "smoke"
        smoke.mkdir()
        # A short CPU-bound native workload checks capture and frame symbolization
        # before the optimized workspace build. It is not a recurring test.
        source = smoke / "smoke.c"
        source.write_text('#include <time.h>\nvolatile unsigned long value;\n'
                          '__attribute__((noinline)) void cpu_profile_smoke(void) {\n'
                          ' clock_t start = clock();\n'
                          ' while (clock() - start < CLOCKS_PER_SEC / 2) {\n'
                          '  for (int i = 0; i < 10000; ++i) value += i;\n'
                          ' }\n}\nint main(void) { cpu_profile_smoke(); return 0; }\n')
        with (smoke / "smoke.log").open("w") as log:
            run(["cc", "-g", "-O1", "-fno-omit-frame-pointer", source, "-o", smoke / "smoke"],
                log, stderr=subprocess.STDOUT)
            if self.args.mode == "stat":
                self.check_events(smoke, log)
            run(self.command(smoke, [smoke / "smoke"]), log, env=self.env, stderr=subprocess.STDOUT)
        self.extract(smoke, "cpu_profile_smoke")

    def check_events(self, smoke, log):
        unavailable = {}
        for event in EVENTS:
            report = smoke / (event + ".txt")
            command = ["perf", "stat", "-e", event, "-o", str(report), "--", "true"]
            result = subprocess.run(command, env=self.env, stdout=log, stderr=log)
            text = report.read_text() if report.exists() else ""
            if "<not supported>" in text or "not supported" in text.lower():
                unavailable[event] = text
            elif result.returncode or "<not counted>" in text:
                raise RuntimeError(f"perf stat preflight failed for {event}; inspect {report} and smoke.log")
            else:
                self.events.append(event)
        self.metadata["unavailable_statistics"] = unavailable
        if not self.events:
            raise RuntimeError("No supported perf stat events; inspect smoke reports")

    def extract(self, directory, expected_symbol="sql_fun_server"):
        if self.args.mode == "stat":
            text = (directory / "stat.txt").read_text()
            if not text.strip() or any(event not in text for event in self.events):
                raise RuntimeError("Incomplete perf stat report")
            if "<not counted>" in text or "<not supported>" in text:
                raise RuntimeError("Selected perf stat events were not measured; inspect stat.txt")
            return
        commands = [
            ["perf", "script", "-i", str(directory / "perf.data"), "--demangle",
             "-F", "comm,pid,tid,time,event,ip,sym,dso"],
            ["c++filt", "--format=rust"],
            ["perl", str(self.args.flamegraph / "stackcollapse-perf.pl"), "--event-filter=cpu-clock:u"],
            ["perl", str(self.args.flamegraph / "flamegraph.pl"), "--title=Server user-space CPU samples",
             "--countname=samples"],
        ]
        # Files keep diagnostics/recovery simple and avoid a failed pipeline
        # stage being hidden by successful SVG rendering.
        names = ("perf.script", "stacks.txt", "stacks.folded", "flamegraph.svg")
        previous = None
        with (directory / "extraction.log").open("w") as errors:
            for command, name in zip(commands, names):
                with (directory / name).open("w") as output:
                    if previous is None:
                        run(command, output, env=self.env, stderr=errors)
                    else:
                        with previous.open() as source:
                            run(command, output, stdin=source, env=self.env, stderr=errors)
                previous = directory / name
        folded = (directory / "stacks.folded").read_text()
        if not folded.strip() or expected_symbol not in folded or "<svg" not in previous.read_text():
            raise RuntimeError("Empty or unsymbolized server profile; inspect extraction.log and perf.data")
        self.metadata.setdefault("extraction_commands", {})[directory.name] = commands
