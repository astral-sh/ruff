FROM rust:1.73 as build

RUN apt update && apt install musl musl-dev musl-tools
RUN rustup target add x86_64-unknown-linux-musl
COPY crates crates
COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock
RUN cargo build --bin ruff --release --target x86_64-unknown-linux-musl
# Optimize binary size
RUN strip --strip-all target/x86_64-unknown-linux-musl/release/ruff

FROM scratch
COPY --from=build target/x86_64-unknown-linux-musl/release/ruff /ruff
WORKDIR /io
ENTRYPOINT ["/ruff"]
