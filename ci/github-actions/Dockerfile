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
    libssl-dev clang \
    postgresql-client libpq-dev \
    && rm -rf /var/lib/apt/lists/*

RUN apt-get update \
    && apt-get install -y --no-install-recommends automake autoconf libtool make pkg-config \
    && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain stable \
    && rustc -V && cargo -V
RUN cargo install --locked --force \
    cargo-binstall

RUN cargo binstall --locked --force --root /opt/tools \
    cargo-llvm-cov mdbook cargo-deny tera-cli taplo-cli cargo-nextest sccache cargo-sort cargo-edit

RUN cargo install --locked --force --root /opt/tools \
    cargo-binstall typos-cli

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
    libssl-dev clang jq \
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

RUN git config --system --add safe.directory '*'

# install uv
RUN curl -LsSf https://astral.sh/uv/install.sh \
    | env UV_UNMANAGED_INSTALL="/usr/local/bin" sh

# install graphify
RUN UV_TOOL_DIR=/opt/uv-tools \
    UV_TOOL_BIN_DIR=/usr/local/bin \
    uv --no-cache tool install graphifyy \
    && graphify --version

COPY --from=builder /opt/tools/bin /usr/local/cargo/bin
COPY --from=builder /opt/serena.tar.gz /opt/serena.tar.gz

COPY resources/serena-wrapper.sh /usr/local/bin/serena
COPY resources/codex-config.toml /usr/local/config-template/codex-config.toml

RUN chmod +x /usr/local/bin/serena
