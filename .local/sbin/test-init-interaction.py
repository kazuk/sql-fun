"""PTY regressions invoked by the installed-CLI extraction validation suite.

All projects, homes and recording tools are test-owned. Server detection uses a
recording psql fixture; real extraction remains covered by the owning suite.
"""
import errno
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import struct
import subprocess
import termios
import time

ESC = b"\x1b"
ENTER = b"\r"
DOWN = b"\x1b[B"
INITIAL = "Initialize sql-fun development support?"
HOME = "sql-fun home directory (SQL_FUN_HOME)?"
ENGINES = "Choose engines for development support"
SOURCE = "Choose the PostgreSQL builtin source"
CONNECTION = "PostgreSQL libpq connection string"
VERSIONS = "Choose the connected PostgreSQL version to initialize"
DOCKER_VERSIONS = "Choose PostgreSQL versions to initialize"
FINAL = "Choose one action to finish initialization"


class Terminal:
    def __init__(self, binary, project, *, existing=False, metadata=False, extra=()):
        self.project = project
        project.mkdir()
        self.home = project / "chosen-home"
        if existing:
            (self.home / "postgres/17").mkdir(parents=True)
            (self.home / "postgres/17/metadata.json").write_text('{"sentinel":true}')
            (self.home / "keep").write_text("user data")
        self.before = self.snapshot()
        self.log = project / "calls.jsonl"
        tools = project / "recording-tools"
        tools.mkdir()
        recorder = '''#!/usr/bin/python3
import json, os, sys
from pathlib import Path
sql = sys.stdin.read() if Path(sys.argv[0]).name == "psql" else ""
with open(os.environ["INIT_CALLS"], "a") as log:
    log.write(json.dumps([Path(sys.argv[0]).name, sys.argv[1:], sql]) + "\\n")
if sql != "SHOW server_version_num; SHOW server_version;\\n":
    sys.exit(91)
version = "18" if "server18" in sys.argv else "17"
sys.stdout.write(version + "0000\\0" + version + ".0\\0")
'''
        for tool in ("psql", "docker"):
            path = tools / tool
            path.write_text(recorder)
            path.chmod(0o755)
        manifest = project / "Cargo.toml"
        manifest.write_text("[package.metadata.database.main]\nengine = 'PostgreSQL'\ndialect = 'postgres'\nversion = '17'\n" if metadata else "not valid TOML [")
        env = dict(os.environ, SQL_FUN_HOME=str(self.home), TERM="xterm", PATH=str(tools), INIT_CALLS=str(self.log))
        for name in ("SQL_FUN_DATASOURCE", "SQL_FUN_METADATA_FILE", "SQL_FUN_DATABASE_URL", "SQL_FUN_POSTGRES_VERSION"):
            env.pop(name, None)
        args = [binary]
        if metadata:
            args += ["--metadata-file", str(manifest)]
        args += ["init", *extra]
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 180, 0, 0))

        def session():
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)

        self.process = subprocess.Popen(args, cwd=project, env=env, stdin=slave,
                                        stdout=slave, stderr=slave, preexec_fn=session)
        os.close(slave)
        self.transcript = bytearray()
        try:
            self.wait(INITIAL)
            self.wait("Ctrl+C: Cancel.")
        except BaseException:
            self.close()
            raise

    def snapshot(self):
        if not self.home.exists():
            return None
        return {str(p.relative_to(self.home)): p.read_bytes() if p.is_file() else None for p in self.home.rglob("*")}

    def rendered(self, since=0):
        return re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]", b"", self.transcript[since:]).decode(errors="replace")

    def read(self, timeout):
        ready, _, _ = select.select([self.master], [], [], timeout)
        if ready:
            try:
                chunk = os.read(self.master, 65536)
            except OSError as error:
                if error.errno == errno.EIO:
                    return False
                raise
            self.transcript.extend(chunk)
            return bool(chunk)
        return True

    def wait(self, marker, since=0):
        deadline = time.monotonic() + 15
        while marker not in self.rendered(since):
            if time.monotonic() >= deadline or not self.read(0.1):
                raise AssertionError(f"missing {marker!r}: {self.rendered(since)!r}")

    def key(self, keys, marker):
        start = len(self.transcript)
        os.write(self.master, keys)
        self.wait(marker, start)
        self.wait("Ctrl+C: Cancel.", start + self.transcript[start:].find(marker.encode()))
        return start

    def stay(self, keys):
        # inquire renders only terminal differences: an unchanged validation error
        # need not be printed again. Drain redraws, then assert the session lives.
        os.write(self.master, keys)
        deadline = time.monotonic() + 5
        while select.select([self.master], [], [], 0.2)[0]:
            assert time.monotonic() < deadline, self.rendered()
            assert self.read(0), self.rendered()
        assert self.process.poll() is None, self.rendered()
        self.unchanged()

    def unchanged(self):
        assert self.snapshot() == self.before, "setup mutated the selected home before Ok"
        calls = [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []
        assert all(tool == "psql" and sql == "SHOW server_version_num; SHOW server_version;\n" for tool, _, sql in calls), calls
        return calls

    def finish(self, keys, status):
        start = len(self.transcript)
        os.write(self.master, keys)
        deadline = time.monotonic() + 15
        while self.process.poll() is None:
            assert time.monotonic() < deadline, self.rendered(start)
            self.read(0.1)
        while self.read(0):
            if not select.select([self.master], [], [], 0)[0]:
                break
        assert self.process.returncode == status, self.rendered(start)
        text = self.rendered(start)
        if "Initialization cancelled." in text:
            assert text.count("Initialization cancelled.") == 1
        assert "Error:" not in text and "Caused by:" not in text, text
        return text

    def close(self):
        if self.process.poll() is None:
            self.process.terminate()
            self.process.wait(timeout=5)
        os.close(self.master)


def route(terminal, *, server=True, metadata=False):
    selection = "Choose datasources to initialize" if metadata else ENGINES
    steps = [(b"y\r", HOME), (ENTER, f"Use {terminal.home}"), (b"y\r", selection),
             (b" \r", SOURCE)]
    if server:
        steps += [(DOWN + ENTER, CONNECTION), (b"server17\r", VERSIONS), (b" \r", FINAL)]
    else:
        steps += [(ENTER, DOCKER_VERSIONS), (b" " + DOWN + b" \r", FINAL)]
    return steps


def validate(binary, destination):
    root = destination / "init-navigation"
    root.mkdir()
    # Cancellation at every question, including the connection and singleton version.
    for index in range(8):
        terminal = Terminal(binary, root / f"cancel-{index}", existing=index % 2 == 1)
        try:
            for keys, marker in route(terminal)[:index]:
                terminal.key(keys, marker)
            terminal.unchanged()
            text = terminal.finish(b"\x03", 130)
            assert "Initialization cancelled." in text
            terminal.unchanged()
        finally:
            terminal.close()

    terminal = Terminal(binary, root / "cancel-datasource", metadata=True, existing=True)
    try:
        for keys, marker in route(terminal, metadata=True)[:3]:
            terminal.key(keys, marker)
        assert "Initialization cancelled." in terminal.finish(b"\x03", 130)
        terminal.unchanged()
    finally:
        terminal.close()

    for server, metadata in ((False, False), (True, True)):
        terminal = Terminal(binary, root / f"round-trip-{server}", metadata=metadata, existing=server)
        try:
            for _ in range(3):
                terminal.key(ESC, INITIAL)
            for keys, marker in route(terminal, server=server, metadata=metadata):
                terminal.key(keys, marker)
            before = terminal.rendered().split("Initialization review")[-1].split(FINAL)[0]
            terminal.unchanged()
            backwards = ([VERSIONS, CONNECTION] if server else [DOCKER_VERSIONS]) + [SOURCE, "Choose datasources to initialize" if metadata else ENGINES, f"Use {terminal.home}", HOME, INITIAL]
            for marker in backwards:
                terminal.key(ESC, marker)
            terminal.key(ESC, INITIAL)
            assert "<canceled>" not in terminal.rendered()
            assert "<back>" in terminal.rendered()
            forwards = [HOME, f"Use {terminal.home}", "Choose datasources to initialize" if metadata else ENGINES, SOURCE] + ([CONNECTION, VERSIONS] if server else [DOCKER_VERSIONS]) + [FINAL]
            for marker in forwards:
                terminal.key(ENTER, marker)
            after = terminal.rendered().split("Initialization review")[-1].split(FINAL)[0]
            assert before == after, (before, after)
            # Enter alone is neither approval nor cancellation, even when repeated.
            terminal.key(ENTER, "Select exactly one action explicitly.")
            terminal.stay(ENTER * 3)
            # Selecting both actions is invalid; leaving and re-entering resets both.
            terminal.stay(b" " + DOWN + b" \r")
            terminal.key(ESC, VERSIONS if server else DOCKER_VERSIONS)
            terminal.key(ENTER, FINAL)
            terminal.key(ENTER, "Select exactly one action explicitly.")
            terminal.unchanged()
            text = terminal.finish(DOWN + b" \r", 0)
            assert "Initialization cancelled." in text
            calls = terminal.unchanged()
            assert len(calls) == (1 if server else 0), calls
        finally:
            terminal.close()

    terminal = Terminal(binary, root / "change-source-connection")
    try:
        for keys, marker in route(terminal, server=False):
            terminal.key(keys, marker)
        terminal.key(ESC, DOCKER_VERSIONS)
        terminal.key(ESC, SOURCE)
        terminal.key(DOWN + ENTER, CONNECTION)
        terminal.key(b"server17\r", VERSIONS)
        terminal.key(ENTER, "Select at least one option explicitly.")
        terminal.key(b" \r", FINAL)
        terminal.key(ESC, VERSIONS)
        terminal.key(ESC, CONNECTION)
        terminal.key(b"\x7f" * len("server17") + b"server18\r", VERSIONS)
        terminal.key(ENTER, "Select at least one option explicitly.")
        terminal.key(b" \r", FINAL)
        assert "Versions: 18" in terminal.rendered().split("Initialization review")[-1]
        calls = terminal.unchanged()
        assert len(calls) == 2
        terminal.finish(b"\x03", 130)
    finally:
        terminal.close()

    # Correct an earlier home and engine choice, then deliberately authorize SQLite.
    terminal = Terminal(binary, root / "correct-home-engine")
    try:
        for keys, marker in route(terminal, server=False):
            terminal.key(keys, marker)
        for marker in (DOCKER_VERSIONS, SOURCE, ENGINES, f"Use {terminal.home}", HOME):
            terminal.key(ESC, marker)
        new_home = terminal.project / "corrected-home"
        terminal.key(b"\x7f" * len(str(terminal.home)) + str(new_home).encode() + ENTER, f"Use {new_home}")
        terminal.key(b"y\r", ENGINES)
        terminal.key(b" " + DOWN + b" \r", FINAL)
        terminal.unchanged()
        assert not new_home.exists()
        terminal.key(ENTER, "Select exactly one action explicitly.")
        text = terminal.finish(b" \r", 0)
        assert "SQLite development-support setup is still under development." in text
        assert new_home.is_dir() and not terminal.home.exists()
        assert not terminal.log.exists()
    finally:
        terminal.close()

    terminal = Terminal(binary, root / "decline")
    try:
        assert "Initialization cancelled." in terminal.finish(b"n\r", 0)
        terminal.unchanged()
    finally:
        terminal.close()
    print("Interactive init: PTY navigation, retention, fresh authorization, cancellation and mutation boundaries passed", flush=True)
