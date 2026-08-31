# syntax=docker/dockerfile:1.7

FROM rust:1.98.0-bookworm@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922

LABEL org.opencontainers.image.source="https://github.com/oborchers/proqi"

RUN rustup toolchain install 1.88.0 --profile minimal

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo install cargo-nextest --version 0.9.143 --locked && \
    cargo install cargo-llvm-cov --version 0.9.0 --locked && \
    cargo install cargo-deny --version 0.20.2 --locked && \
    cargo install cargo-audit --version 0.22.2 --locked && \
    cargo install cargo-shear --version 1.11.2 --locked && \
    cargo install cargo-about --version 0.9.2 --locked --features cli

ARG NFPM_VERSION=2.47.0
ARG TARGETARCH
ARG NFPM_AMD64_SHA256=0660ca602b2d2d2ae4781a06c692b3eeb9d437ffea05b831d76e41f4a3188783
ARG NFPM_ARM64_SHA256=1c0f5f143e4df95dfca9be489384819483c8252320640459313db800503d919c
RUN archive=/tmp/nfpm.tar.gz && \
    case "$TARGETARCH" in \
      amd64) nfpm_arch=x86_64; NFPM_SHA256="$NFPM_AMD64_SHA256" ;; \
      arm64) nfpm_arch=arm64; NFPM_SHA256="$NFPM_ARM64_SHA256" ;; \
      *) echo "unsupported target architecture: $TARGETARCH" >&2; exit 1 ;; \
    esac && \
    curl \
      --proto '=https' \
      --tlsv1.2 \
      --fail \
      --location \
      --retry 3 \
      --output "$archive" \
      "https://github.com/goreleaser/nfpm/releases/download/v${NFPM_VERSION}/nfpm_${NFPM_VERSION}_Linux_${nfpm_arch}.tar.gz" && \
    printf '%s  %s\n' "$NFPM_SHA256" "$archive" | sha256sum --check --strict && \
    tar --extract --gzip --file "$archive" --directory /usr/local/bin nfpm && \
    chmod 0755 /usr/local/bin/nfpm && \
    rm "$archive" && \
    nfpm --version

COPY run.sh /usr/local/bin/proqi-ci-linux
RUN chmod 0755 /usr/local/bin/proqi-ci-linux

ENTRYPOINT ["proqi-ci-linux"]
