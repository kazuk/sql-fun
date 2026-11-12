"""Private, unprivileged Ubuntu package provisioning for one profiling run."""
import hashlib
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import time


def run(command, log, **kwargs):
    print(f"+ {shlex.join(map(str, command))}", flush=True)
    errors = kwargs.pop("stderr", subprocess.STDOUT)
    process = subprocess.Popen(list(map(str, command)), stdout=log, stderr=errors,
                               start_new_session=True, **kwargs)
    try:
        status = process.wait()
        if status:
            raise subprocess.CalledProcessError(status, command)
    except BaseException:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
        raise


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def provision(scratch, reports):
    began = time.monotonic()
    apt = scratch / "apt"
    for name in ("lists/partial", "archives/partial", "etc/parts", "etc/preferences.d", "logs"):
        (apt / name).mkdir(parents=True)
    # A separate configuration tree prevents host APT hooks/cache mutations.
    (apt / "status").touch()
    (apt / "etc/apt.conf").touch()
    (apt / "etc/sources.list").write_text(
        "deb [signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] "
        "http://archive.ubuntu.com/ubuntu noble main universe\n"
        "deb [signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] "
        "http://archive.ubuntu.com/ubuntu noble-updates main universe\n"
        "deb [signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] "
        "http://security.ubuntu.com/ubuntu noble-security main universe\n")
    config = apt / "private.conf"
    config.write_text("\n".join([
        f'Dir::Etc "{apt / "etc"}";',
        'Dir::Etc::parts "parts";', 'Dir::Etc::main "apt.conf";',
        'Dir::Etc::sourcelist "sources.list";', 'Dir::Etc::sourceparts "-";',
        f'Dir::State::lists "{apt / "lists"}";',
        f'Dir::State::status "{apt / "status"}";',
        f'Dir::State::extended_states "{apt / "extended_states"}";',
        f'Dir::Cache::archives "{apt / "archives"}";',
        f'Dir::Cache::pkgcache "{apt / "pkgcache.bin"}";',
        f'Dir::Cache::srcpkgcache "{apt / "srcpkgcache.bin"}";',
        f'Dir::Log "{apt / "logs"}";',
        'APT::Sandbox::User "' + str(os.getuid()) + '";',
        'APT::Get::List-Cleanup "false";',
    ]) + "\n")
    env = dict(os.environ, APT_CONFIG=str(config), LC_ALL="C")
    with (reports / "provision.log").open("w") as log:
        run(["apt-get", "update", "--error-on=any"], log, env=env)
        # Empty private status requests the entire runtime dependency closure,
        # even when heaptrack already happens to be installed on the host.
        run(["apt-get", "install", "--download-only", "--no-install-recommends",
             "--yes", "heaptrack", "libheaptrack", "binutils"], log, env=env)
        root = scratch / "tools"
        root.mkdir()
        packages = []
        for package in sorted((apt / "archives").glob("*.deb")):
            identity = subprocess.check_output(
                ["dpkg-deb", "--show", str(package), "${Package}\t${Version}\t${Architecture}"], text=True)
            packages.append(dict(identity=identity, file=package.name, sha256=digest(package)))
            run(["dpkg-deb", "--extract", package, root], log)
    (reports / "packages.json").write_text(json.dumps(dict(
        sources=(apt / "etc/sources.list").read_text(), packages=packages,
        seconds=time.monotonic() - began), indent=2) + "\n")
    return Tools(root)


class Tools:
    def __init__(self, root):
        self.root = root
        self.heaptrack = root / "usr/bin/heaptrack"
        self.printer = root / "usr/bin/heaptrack_print"
        self.demangler = root / "usr/bin/c++filt"
        triplet = subprocess.check_output(["dpkg-architecture", "-qDEB_HOST_MULTIARCH"], text=True).strip()
        self.env = dict(os.environ, LC_ALL="C", LD_LIBRARY_PATH=":".join(
            str(root / path) for path in (f"usr/lib/{triplet}", f"lib/{triplet}", "usr/lib", "lib")))
        # No ambient instrumentation, APT configuration or launch overrides.
        for key in ("LD_PRELOAD", "DUMP_HEAPTRACK_OUTPUT", "APT_CONFIG"):
            self.env.pop(key, None)

    def check(self, scratch, reports):
        versions = {}
        for tool in (self.heaptrack, self.printer, self.demangler):
            versions[tool.name] = subprocess.check_output([str(tool), "--version"], env=self.env, text=True).strip()
        help_text = subprocess.check_output([str(self.printer), "--help"], env=self.env, text=True)
        for flag in ("merge-backtraces", "print-peaks", "print-allocators", "print-temporary",
                     "print-leaks", "peak-limit", "disable-builtin-suppressions", "disable-embedded-suppressions"):
            if flag not in help_text:
                raise RuntimeError(f"Provisioned heaptrack_print does not support {flag}")
        (reports / "tools.json").write_text(json.dumps(versions, indent=2) + "\n")
        (reports / "heaptrack-print-help.txt").write_text(help_text)
        demangled = subprocess.check_output([str(self.demangler), "--format=rust"],
                                            input="_RNvC4test4main\n", env=self.env, text=True)
        if demangled.strip() not in ("test::main", "test[0]::main"):
            raise RuntimeError("Provisioned c++filt does not support Rust v0 symbols")
        # Check relocation, symbolization, compression and finalization before
        # spending time on the real release build and full regression workload.
        source = scratch / "smoke.c"
        source.write_text('#include <stdlib.h>\nvoid *volatile saved;\n'
                          '__attribute__((noinline)) void profile_smoke(void) { saved = malloc(4096); }\n'
                          'int main(void) { profile_smoke(); free(saved); return 0; }\n')
        with (reports / "smoke.log").open("w") as log:
            run(["cc", "-g", "-O0", source, "-o", scratch / "smoke"], log)
            run([self.heaptrack, "--record-only", "--output", scratch / "smoke-profile",
                 scratch / "smoke"], log, env=self.env)
        profiles = list(scratch.glob("smoke-profile.*"))
        if len(profiles) != 1:
            raise RuntimeError("Smoke capture did not produce exactly one compressed profile")
        text = subprocess.check_output([str(self.printer), str(profiles[0])], env=self.env, text=True)
        if "profile_smoke" not in text or "calls to allocation functions:" not in text:
            raise RuntimeError("Private heaptrack smoke capture is not symbolized/complete")
        (reports / "smoke-report.txt").write_text(text)
        profiles[0].unlink()

    def extract(self, profile, report):
        command = [str(self.printer), str(profile), "--merge-backtraces=false",
                   "--print-peaks=true", "--print-allocators=true", "--print-temporary=true",
                   "--print-leaks=true", "--peak-limit=20", "--disable-builtin-suppressions",
                   "--disable-embedded-suppressions"]
        temporary = report.with_suffix(".partial")
        with temporary.open("w") as output:
            output.write("Server heap profile. Native heaptrack 'leaks' mean unfreed at capture end,\n"
                         "not proven leaks. Suppressions disabled. Peak bytes are not cumulative bytes.\n"
                         "Cumulative allocated bytes and byte rankings: unavailable in this printer.\n\n")
            output.flush()
            with report.with_suffix(".stderr").open("w") as errors:
                run(command, output, env=self.env, stderr=errors)
        # Keep heaptrack's text layout and file/line information. Ubuntu's
        # printer cannot demangle current Rust v0 names; use the native binutils
        # filter, not a custom profile parser or a compiler configuration change.
        demangle_command = [str(self.demangler), "--format=rust"]
        readable = report.with_suffix(".demangled")
        with temporary.open("r") as source, readable.open("w") as output:
            run(demangle_command, output, stdin=source, env=self.env)
        text = readable.read_text()
        for section in ("MOST CALLS TO ALLOCATION FUNCTIONS", "PEAK MEMORY CONSUMERS",
                        "MOST TEMPORARY ALLOCATIONS", "MEMORY LEAKS", "calls to allocation functions:",
                        "temporary memory allocations:", "peak heap memory consumption:", "total memory leaked:"):
            if section not in text:
                raise RuntimeError(f"Incomplete heaptrack report: missing {section}")
        if "sql_fun_server::" not in text and "sql_fun_server[" not in text:
            raise RuntimeError("No symbolized sql_fun_server frames in the report; retaining raw capture")
        readable.replace(report)
        temporary.unlink()
        return dict(printer=command, demangler=demangle_command)
