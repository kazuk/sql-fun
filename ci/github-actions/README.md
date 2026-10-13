# CI custom image for GitHub actions for `sql-fun`

See [tool release instructions](tool-releases.md) for prerequisites, direct
Docker build arguments, integrity checks, and published version review.

Run `ci/github-actions/check` with `GIT_ROOT` set to the repository root to
build the image and open an interactive container. After building, it prints
the uncompressed local image size in MiB and bytes, plus the size change when
an existing `ci-builder:local` image was available before the build. This is
not the compressed registry download size or the build cache disk usage.

- `--build-only`: build and report size without creating volumes or starting a
  container.
- `--rebuild`: pull the base image and build without the layer cache.
- `--clean`: remove the target volume before starting the container; ignored
  with `--build-only`.

To compare sizes, run `check --build-only` before and after editing the
Dockerfile. Use `docker history ci-builder:local` to inspect individual layer
sizes.

## PostgreSQL development oracles

Interactive launches require the Docker Compose plugin on the **host**, with
support for `docker compose up --wait --wait-timeout`. Install or update it using
[Docker's Compose plugin instructions](https://docs.docker.com/compose/install/).
Check `docker compose version` and `docker compose up --help` before launching.
The builder uses its existing PostgreSQL client; it has no Docker socket or
Docker Engine management access. `--build-only` does not require Compose.

The normal launcher starts four independent, disposable PostgreSQL servers and
waits up to 120 seconds for authenticated SQL healthchecks before opening the
builder shell. The first launch downloads the official PostgreSQL images;
subsequent launches reuse locally available images. No database ports are
published to the host.

| Hostname inside builder | Port | PostgreSQL major |
| --- | --- | --- |
| `postgres15` | 5432 | 15 |
| `postgres16` | 5432 | 16 |
| `postgres17` | 5432 | 17 |
| `postgres18` | 5432 | 18 |

Each instance has role `oracle`, database `oracle`, and password
`sql-fun-development-only`. These are disposable development credentials, not
production credentials. The builder receives matching `PGUSER`, `PGDATABASE`,
and `PGPASSWORD`; select the hostname explicitly. For example, inside builder:

```sh
psql -h postgres15
psql -X -w -h postgres16 -Atc "SELECT current_setting('server_version_num')"
psql -X -w -h postgres17 -Atc "SELECT current_setting('server_version_num')"
psql -X -w -h postgres18 -Atc "SELECT current_setting('server_version_num')"
```

Both local and network server connections require password authentication.
Oracle data resides in separate tmpfs mounts and disappears when the session is
removed. Normal tests can still run without these services; they are optional
behavioral references, not application runtime or unit-test dependencies.

Each invocation prints a unique Compose project name. Shell exit, startup
failure, and handled interruption trigger cleanup of that session's services
and network. The external `user_home` and `ci_target` volumes survive cleanup;
`--clean` explicitly resets `ci_target`. Concurrent sessions have independent
servers but share the existing builder volumes; avoid `--clean` while another
session uses the target volume.

An uncatchable termination (for example, killing the launcher with SIGKILL) can
leave resources running. From the host, explicitly select the project name
printed by that invocation and remove it:

```sh
export BUILDER_UID="$(id -u)" BUILDER_GID="$(id -g)"
compose_file="$GIT_ROOT/ci/github-actions/compose.yaml"
docker compose --env-file /dev/null --file "$compose_file" \
  --project-name <printed-project-name> down --volumes --timeout 10
```

Replace `<printed-project-name>` with the actual session name. External builder
volumes are preserved even with `down --volumes`. Reopen the environment through
`ci/github-actions/check` after updating the launcher or Compose configuration
so the running session uses the new configuration.
