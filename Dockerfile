FROM --platform=$BUILDPLATFORM ubuntu AS build
ENV HOME="/root"
WORKDIR $HOME

RUN apt update && apt install -y build-essential curl python3-venv

# Setup zig as cross compiling linker
RUN python3 -m venv $HOME/.venv
RUN .venv/bin/pip install cargo-zigbuild
ENV PATH="$HOME/.venv/bin:$PATH"

# Install rust
ARG TARGETPLATFORM
RUN case "$TARGETPLATFORM" in \
    "linux/arm64") echo "aarch64-unknown-linux-musl" > rust_target.txt ;; \
    "linux/amd64") echo "x86_64-unknown-linux-musl" > rust_target.txt ;; \
    *) exit 1 ;; \
    esac
COPY rust-toolchain.toml rust-toolchain.toml
ARG RUSTUP_VERSION=1.29.2
RUN curl --proto '=https' --tlsv1.2 -sSf \
    "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/$(uname -m)-unknown-linux-gnu/rustup-init" \
    -o rustup-init \
    && chmod +x rustup-init \
    && ./rustup-init -y --target $(cat rust_target.txt) --profile minimal --default-toolchain none \
    && rm rustup-init
ENV PATH="$HOME/.cargo/bin:$PATH"
# Installs the correct toolchain version from rust-toolchain.toml and then the musl target
RUN rustup target add $(cat rust_target.txt)

# Build
COPY crates crates
COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock
RUN cargo zigbuild --bin ruff --target $(cat rust_target.txt) --release
RUN cp target/$(cat rust_target.txt)/release/ruff /ruff
# TODO: Optimize binary size, with a version that also works when cross compiling
# RUN strip --strip-all /ruff

FROM scratch
COPY --from=build /ruff /ruff
WORKDIR /io
ENTRYPOINT ["/ruff"]
