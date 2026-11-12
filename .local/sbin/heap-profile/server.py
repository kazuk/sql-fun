"""Profiling-only ownership and lifecycle; the CI server runner is unchanged."""
import json
import os
from pathlib import Path
import signal
import subprocess
import time

from tools import run


class ProfiledServer:
    def __init__(self, tools, binary, runtime, raw, reports, env):
        self.tools, self.binary, self.runtime = tools, binary, runtime
        self.raw, self.reports, self.env = raw, reports, env
        self.process = None
        self.server_pid = None
        self.registration = None
        self.finished = False

    def start(self):
        launcher = self.raw / "launch-server"
        source = self.raw / "launch-server.c"
        # heaptrack clears LD_PRELOAD/DUMP_HEAPTRACK_OUTPUT in a dynamic
        # launcher's allocator hook. A static launcher leaves them intact until
        # exec starts the actual server. It also preserves the registered PID.
        source.write_text('#include <stdio.h>\n#include <unistd.h>\n'
                          'int main(int argc, char **argv) {\n'
                          '  if (argc < 3) return 1;\n'
                          '  FILE *file = fopen(argv[1], "w");\n'
                          '  if (!file) return 1;\n'
                          '  if (fprintf(file, "%ld\\n", (long)getpid()) < 0) return 1;\n'
                          '  if (fclose(file)) return 1;\n'
                          '  execv(argv[2], argv + 2);\n'
                          '  perror("exec server"); return 1;\n}\n')
        with (self.reports / "launcher-build.log").open("w") as log:
            run(["cc", "-static", "-O2", source, "-o", launcher], log)
        self.log = (self.reports / "server.log").open("w")
        self.process = subprocess.Popen([
            str(self.tools.heaptrack), "--record-only", "--output", str(self.raw / "server-profile"),
            str(launcher), str(self.raw / "server.pid"), str(self.binary),
            "--server-idle-timeout", "180"],
            env=dict(self.env, LD_LIBRARY_PATH=self.tools.env["LD_LIBRARY_PATH"]), stdin=subprocess.DEVNULL,
            stdout=self.log, stderr=subprocess.STDOUT, start_new_session=True)
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError("Profiler/server exited before readiness; inspect server.log")
            try:
                self.server_pid = int((self.raw / "server.pid").read_text())
                registration = (self.runtime / "sql-fun-server.pid").read_bytes()
                if json.loads(registration)["pid"] != self.server_pid:
                    raise RuntimeError("Registration does not identify the owned profiled server")
                self.registration = registration
                self.verify()
                return
            except (FileNotFoundError, ValueError):
                time.sleep(0.05)
        raise RuntimeError("Profiled server readiness timed out")

    def verify(self):
        if self.process.poll() is not None:
            raise RuntimeError("Owned profiler exited during the suite")
        # The private launcher execs the server in heaptrack's process group.
        # Check both ancestry and group, not an arbitrary PID from registration.
        if os.getpgid(self.server_pid) != self.process.pid:
            raise RuntimeError("Profiled server is not in the owned process group")
        pid = self.server_pid
        while pid != self.process.pid:
            fields = (Path("/proc") / str(pid) / "stat").read_text().rsplit(")", 1)[1].split()
            if fields[0] == "Z":
                raise RuntimeError("Owned server exited during the suite")
            pid = int(fields[1])
            if pid <= 1:
                raise RuntimeError("Profiled server is not a descendant of heaptrack")
        if (self.runtime / "sql-fun-server.pid").read_bytes() != self.registration:
            raise RuntimeError("Owned profiled server registration changed")

    def finish(self):
        print("Waiting for the server's 180-second idle shutdown and profile finalization…", flush=True)
        status = self.process.wait(timeout=240)
        if status != 0:
            raise RuntimeError(f"Profiler/server failed with status {status}")
        self.log.close()
        profiles = list(self.raw.glob("server-profile.*"))
        if len(profiles) != 1 or not profiles[0].stat().st_size:
            raise RuntimeError("Expected exactly one nonempty compressed server profile")
        self.finished = True
        return profiles[0]

    def close(self):
        if self.process is not None and not self.finished:
            # Even if the profiler exited, its unreaped group may contain a
            # server/interpreter. Group ownership was established at Popen.
            try:
                os.killpg(self.process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            self.process.wait()
        if hasattr(self, "log"):
            self.log.close()
