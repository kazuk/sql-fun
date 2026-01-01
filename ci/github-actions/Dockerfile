# syntax=docker/dockerfile:1

FROM ubuntu:24.04 AS builder

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

RUN sudo apt-get update \
 && sudo apt-get install -y automake autoconf libtool make pkg-config

RUN curl -fsSL https://sh.rustup.rs \
 | sh -s -- -y --profile minimal --default-toolchain stable \
  && rustc -V && cargo -V

RUN cargo install --locked --force --root /opt/tools \
  cargo-llvm-cov mdbook cargo-deny tera-cli taplo-cli cargo-nextest

FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:/usr/local/rustup/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

RUN apt-get update && apt-get install -y --no-install-recommends \
  curl sudo

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
  && rm -rf /var/lib/apt/lists/*

# Rust (stable)
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable \
  && rustc -V && cargo -V

COPY --from=builder /opt/tools/bin /usr/local/cargo/bin
