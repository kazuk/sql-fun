# syntax=docker/dockerfile:1

FROM ubuntu:24.04 AS builder

ARG SERENA_TAG
ARG SERENA_DOWNLOAD_BASE="https://github.com/oraios/serena/archive/refs/tags/"

ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:/usr/local/rustup/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
    ca-certificates curl wget git openssh-client \
    unzip zip xz-utils sudo \
    build-essential pkg-config \
    libssl-dev clang python3 \
    postgresql-client libpq-dev \
    && rm -rf /var/lib/apt/lists/*

RUN apt-get update \
    && apt-get install -y --no-install-recommends automake autoconf libtool make pkg-config \
    && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain stable \
    && rustc -V && cargo -V
ARG CARGO_BINSTALL_TAG
ARG CARGO_BINSTALL_SHA256

RUN test "$(dpkg --print-architecture)" = amd64 \
    && test -n "${CARGO_BINSTALL_TAG}" && test -n "${CARGO_BINSTALL_SHA256}" \
    && mkdir -p /tmp/binstall /opt/tools/bin \
    && curl -fsSL -o /tmp/binstall/archive.tgz \
    "https://github.com/cargo-bins/cargo-binstall/releases/download/${CARGO_BINSTALL_TAG}/cargo-binstall-x86_64-unknown-linux-musl.tgz" \
    && printf '%s  %s\n' "${CARGO_BINSTALL_SHA256}" /tmp/binstall/archive.tgz | sha256sum --check --strict - \
    && tar -xzf /tmp/binstall/archive.tgz -C /tmp/binstall cargo-binstall \
    && install -m 0755 /tmp/binstall/cargo-binstall /usr/local/cargo/bin/cargo-binstall \
    && install -m 0755 /tmp/binstall/cargo-binstall /opt/tools/bin/cargo-binstall \
    && test "$(cargo binstall -V)" = "${CARGO_BINSTALL_TAG#v}" \
    && rm -rf /tmp/binstall

RUN cargo binstall --locked --force --root /opt/tools \
    cargo-llvm-cov mdbook cargo-deny tera-cli taplo-cli cargo-nextest sccache cargo-sort cargo-edit cargo-expand cargo-deb

# #826: a same-path struct does not replace a trait that is still public API.
# Keep the upstream lockfile and package version; identify the corrected CLI.
RUN <<'SH'
set -eu
mkdir -p /tmp/semver-source
curl -fsSL https://static.crates.io/crates/cargo-semver-checks/cargo-semver-checks-0.51.0.crate \
    -o /tmp/semver-source/source.crate
printf '%s  %s\n' \
    484e65e62077aed1a119d04ab414038d20b9013302b7353487ff69a461842bf8 \
    /tmp/semver-source/source.crate | sha256sum --check --strict -
tar -xzf /tmp/semver-source/source.crate -C /tmp/semver-source
cd /tmp/semver-source/cargo-semver-checks-0.51.0
python3 - <<'PYTHON'
from pathlib import Path

def replace_once(path, before, after):
    source = path.read_text()
    if source.count(before) != 1:
        raise ValueError(f"unexpected pinned analyzer source: {path}")
    path.write_text(source.replace(before, after))

query = Path("src/lints/trait_changed_kind.ron")
# Reuse trait_missing's zero-count predicate at the baseline import path.
replace_once(query, '            current {\n', '''            current {
                item @fold @transform(op: "count") @filter(op: "=", value: ["$zero"]) {
                    ... on Trait {
                        visibility_limit @filter(op: "=", value: ["$public"])
                        importable_path {
                            path @filter(op: "=", value: ["%path"])
                            public_api @filter(op: "=", value: ["$true"])
                        }
                    }
                }
''')
replace_once(query, '        "public": "public",', '        "public": "public",\n        "zero": 0,')
replace_once(Path("src/main.rs"), '#[command(version, propagate_version = true)]',
             '#[command(version = "0.51.0-sqlfun.1", propagate_version = true)]')
PYTHON
cargo install --path . --locked --root /opt/tools
analyzer=/opt/tools/bin/cargo-semver-checks
test "$("$analyzer" --version)" = 'cargo-semver-checks 0.51.0-sqlfun.1'

# Exercise the compiled query, then verify a real replacement still fails.
mkdir -p /tmp/semver-probe/src
cd /tmp/semver-probe
cat > Cargo.toml <<'TOML'
[package]
name = "semver-image-probe"
version = "1.0.0"
edition = "2021"
TOML
cat > src/lib.rs <<'RUST'
mod traits { pub trait Api {} }
mod generated {
    mod inner { pub struct Api(Option<u8>); }
    pub use self::inner::Api;
    pub fn keep(_: Api) {}
}
pub mod api { pub use crate::traits::Api; pub use crate::generated::*; }
RUST
RUSTC_BOOTSTRAP=1 cargo rustdoc --offline --lib -- -Z unstable-options --output-format json
cp target/doc/semver_image_probe.json old.json
"$analyzer" check-release --baseline-rustdoc old.json --current-rustdoc old.json --release-type patch
printf '%s\n' 'pub mod api { pub struct Api; }' > src/lib.rs
RUSTC_BOOTSTRAP=1 cargo rustdoc --offline --lib -- -Z unstable-options --output-format json
status=0
"$analyzer" check-release --baseline-rustdoc old.json \
    --current-rustdoc target/doc/semver_image_probe.json --release-type patch \
    --color never > result.log 2>&1 || status=$?
cat result.log
test "$status" -eq 100
grep -q -- 'failure trait_changed_kind:' result.log
rm -rf /tmp/semver-source /tmp/semver-probe
SH

ARG TYPOS_TAG
ARG TYPOS_SHA256

RUN test -n "${TYPOS_TAG}" && test -n "${TYPOS_SHA256}" \
    && mkdir -p /tmp/typos \
    && curl -fsSL -o /tmp/typos/archive.tar.gz \
    "https://github.com/crate-ci/typos/releases/download/${TYPOS_TAG}/typos-${TYPOS_TAG}-x86_64-unknown-linux-musl.tar.gz" \
    && printf '%s  %s\n' "${TYPOS_SHA256}" /tmp/typos/archive.tar.gz | sha256sum --check --strict - \
    && tar -xzf /tmp/typos/archive.tar.gz -C /tmp/typos ./typos \
    && install -m 0755 /tmp/typos/typos /opt/tools/bin/typos \
    && test "$(/opt/tools/bin/typos --version)" = "typos-cli ${TYPOS_TAG#v}" \
    && rm -rf /tmp/typos

# Match the Scrut release used by installed-package contract/E2E jobs.
RUN mkdir -p /tmp/scrut \
    && curl -fsSL -o /tmp/scrut/dotslash.tar.gz \
    "https://github.com/facebook/dotslash/releases/download/v0.5.9/dotslash-linux-musl.x86_64.v0.5.9.tar.gz" \
    && printf '%s  %s\n' \
    "4c75c6eb7890ae35993b962073f6d9bbe78b42b81a5691303ad70f63bfbf7196" \
    /tmp/scrut/dotslash.tar.gz | sha256sum --check --strict - \
    && tar -xzf /tmp/scrut/dotslash.tar.gz -C /tmp/scrut \
    && curl -fsSL -o /tmp/scrut/scrut.dotslash \
    "https://github.com/facebookincubator/scrut/releases/download/v0.4.3/scrut" \
    && printf '%s  %s\n' \
    "d2e4d5d36198f889dd7f827c29cc1b8cb2a362df98e47cdf84b5ee0e207151fc" \
    /tmp/scrut/scrut.dotslash | sha256sum --check --strict - \
    && scrut_binary=$(/tmp/scrut/dotslash -- fetch /tmp/scrut/scrut.dotslash) \
    && install -m 0755 "$scrut_binary" /opt/tools/bin/scrut \
    && test "$(/opt/tools/bin/scrut --version)" = "scrut 0.4.3" \
    && rm -rf /tmp/scrut /root/.cache/dotslash

# https://github.com/oraios/serena/archive/refs/tags/v0.1.4.tar.gz
RUN curl -fsSL -o /opt/serena.tar.gz \
    "${SERENA_DOWNLOAD_BASE}${SERENA_TAG}.tar.gz"

FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:/usr/local/rustup/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl sudo \
    && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
    | sudo dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg

RUN chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
    | sudo tee /etc/apt/sources.list.d/github-cli.list

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl direnv git gh \
    build-essential pkg-config \
    unzip zip xz-utils sudo \
    libssl-dev clang jq python3 \
    postgresql-client libpq-dev \
    autoconf automake libtool libtool-bin bsdextrautils \
    && rm -rf /var/lib/apt/lists/*

# Rust (stable)
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable \
    && rustup toolchain install nightly --profile minimal \
    && rustup component add --toolchain stable rustfmt clippy llvm-tools-preview rust-analyzer rust-src \
    && rustc -V && cargo -V

# Node.js 22 (for Codex CLI + other MCP tools)
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl gnupg \
    && mkdir -p /etc/apt/keyrings \
    && curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key \
    | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg \
    && echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_22.x nodistro main" \
    > /etc/apt/sources.list.d/nodesource.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends nodejs \
    && node -v && npm -v \
    && rm -rf /var/lib/apt/lists/*

# Codex CLI
RUN npm i -g @openai/codex@latest \
    && codex --version \
    && npm cache clean --force

RUN curl -fsSL https://github.com/agent-ecosystem/skill-validator/releases/download/v1.6.2/skill-validator_1.6.2_linux_amd64.tar.gz \
  | tar -xz -C /usr/local/bin skill-validator

RUN git config --system --add safe.directory '*'

# install uv
RUN curl -LsSf https://astral.sh/uv/install.sh \
    | env UV_UNMANAGED_INSTALL="/usr/local/bin" sh

# install graphify
RUN UV_TOOL_DIR=/opt/uv-tools \
    UV_TOOL_BIN_DIR=/usr/local/bin \
    uv --no-cache tool install graphifyy \
    && graphify --version

RUN curl -LsSf https://github.com/rvben/rumdl/releases/download/v0.2.76/rumdl-v0.2.76-x86_64-unknown-linux-gnu.tar.gz \
    | tar xzf - -C /usr/local/bin \
    && rumdl --version

COPY --from=builder /opt/tools/bin /usr/local/cargo/bin

RUN cargo deb --version \
    && test "$(cargo semver-checks --version)" = 'cargo-semver-checks 0.51.0-sqlfun.1'

COPY --from=builder /opt/serena.tar.gz /opt/serena.tar.gz

COPY resources/serena-wrapper.sh /usr/local/bin/serena
COPY resources/codex-config.toml /usr/local/config-template/codex-config.toml

RUN chmod +x /usr/local/bin/serena
