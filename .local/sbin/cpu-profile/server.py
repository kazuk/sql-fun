"""Own one perf-wrapped server and let perf finalize before releasing it."""
import json
import os
from pathlib import Path
import signal
import subprocess
import time


IDLE_TIMEOUT = 10
FINALIZATION_TIMEOUT = 70


class ProfiledServer:
    def __init__(self, tools, binary, runtime, reports, env, metadata):
        self.tools, self.binary, self.runtime = tools, binary, runtime
        self.reports, self.env, self.metadata = reports, env, metadata
        self.process = None
        self.server_pid = None
        self.finished = False

    def start(self):
        command = self.tools.command(self.reports, [self.binary, "--server-idle-timeout", str(IDLE_TIMEOUT)])
        self.metadata["capture_command"] = command
        self.log = (self.reports / "server.log").open("w")
        self.process = subprocess.Popen(command, env=self.env, stdin=subprocess.DEVNULL,
                                        stdout=self.log, stderr=subprocess.STDOUT, start_new_session=True)
        self.metadata["perf_pid"] = self.process.pid
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError("perf/server exited before readiness; inspect server.log")
            try:
                registration = (self.runtime / "sql-fun-server.pid").read_bytes()
                self.server_pid = json.loads(registration)["pid"]
                self.registration = registration
                self.verify()
                self.metadata.update(server_pid=self.server_pid, server_ready=True)
                return
            except (FileNotFoundError, ValueError):
                time.sleep(0.05)
        raise RuntimeError("Profiled server readiness timed out")

    def verify(self):
        if self.process.poll() is not None:
            raise RuntimeError("Owned perf/server exited during the suite")
        if os.getpgid(self.server_pid) != self.process.pid:
            raise RuntimeError("Registered server is outside the owned process group")
        if (Path("/proc") / str(self.server_pid) / "exe").resolve() != self.binary.resolve():
            raise RuntimeError("Registered server executable differs from the profiling build")
        pid = self.server_pid
        while pid != self.process.pid:
            fields = (Path("/proc") / str(pid) / "stat").read_text().rsplit(")", 1)[1].split()
            if fields[0] == "Z":
                raise RuntimeError("Owned server exited during the suite")
            pid = int(fields[1])
            if pid <= 1:
                raise RuntimeError("Registered server is not a descendant of owned perf")
        if (self.runtime / "sql-fun-server.pid").read_bytes() != self.registration:
            raise RuntimeError("Owned server registration changed")

    def finish(self):
        print(f"Waiting for {IDLE_TIMEOUT}-second server idle shutdown and perf finalization…", flush=True)
        began = time.monotonic()
        status = self.process.wait(timeout=FINALIZATION_TIMEOUT)
        self.metadata["perf_exit"] = status
        self.metadata["server_stopped"] = not (Path("/proc") / str(self.server_pid)).exists()
        self.metadata["capture_complete"] = status == 0 and self.metadata["server_stopped"]
        self.metadata["seconds"]["finalization"] = time.monotonic() - began
        self.finished = self.metadata["capture_complete"]
        if status:
            raise RuntimeError(f"perf/server failed with status {status}; inspect server.log")
        if not self.metadata["server_stopped"]:
            raise RuntimeError("perf exited without reaping the owned server; capture incomplete")

    def close(self):
        if self.process is not None and not self.finished:
            self.metadata["capture_complete"] = False
            # Leave perf alive to drain buffers after the owned server exits.
            # Signal only a server whose registration/ancestry we verified.
            if self.server_pid is not None:
                try:
                    self.verify()
                    os.kill(self.server_pid, signal.SIGTERM)
                except (OSError, RuntimeError):
                    pass
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(self.process.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                try:
                    self.process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(self.process.pid, signal.SIGKILL)
                    self.process.wait()
                    self.metadata["forced_cleanup"] = True
            # Even if perf exited first, its private group can still contain
            # an orphan server. Release that group before returning failure.
            try:
                os.killpg(self.process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            self.metadata["perf_exit"] = self.process.returncode
        if hasattr(self, "log"):
            self.log.close()
