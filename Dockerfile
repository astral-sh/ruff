FROM --platform=$BUILDPLATFORM ubuntu as build
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
# TODO: --default-toolchain none and use rust-toolchain.toml instead
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --target $(cat rust_target.txt) --profile minimal
ENV PATH="$HOME/.cargo/bin:$PATH"

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
