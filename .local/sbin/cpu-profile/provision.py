"""Prepare private Ubuntu profiling tools without changing system packages."""
import json
import os
import subprocess
import time

from tools import digest, run


FLAMEGRAPH_REVISION = "41fee1f99f9276008b7cd112fca19dc3ea84ac32"


def provision(scratch, reports, mode):
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
        # even when profiling tools already happen to be installed on the host.
        run(["apt-get", "install", "--download-only", "--no-install-recommends",
             "--yes", "linux-tools-generic", *(["binutils", "perl"] if mode == "record" else [])], log, env=env)
        root = scratch / "tools"
        root.mkdir()
        packages = []
        for package in sorted((apt / "archives").glob("*.deb")):
            identity = subprocess.check_output(
                ["dpkg-deb", "--show", str(package), "${Package}\t${Version}\t${Architecture}"], text=True)
            packages.append(dict(identity=identity, file=package.name, sha256=digest(package)))
            run(["dpkg-deb", "--extract", package, root], log)
        flamegraph = None
        if mode == "record":
            flamegraph = scratch / "FlameGraph"
            run(["git", "init", flamegraph], log)
            run(["git", "-C", flamegraph, "fetch", "--depth=1",
                 "https://github.com/brendangregg/FlameGraph.git", FLAMEGRAPH_REVISION], log)
            run(["git", "-C", flamegraph, "checkout", "--detach", "FETCH_HEAD"], log)
    (reports / "packages.json").write_text(json.dumps(dict(
        sources=(apt / "etc/sources.list").read_text(), packages=packages,
        seconds=time.monotonic() - began), indent=2) + "\n")
    return root, flamegraph

