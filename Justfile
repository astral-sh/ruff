export RUFF_UPDATE_SCHEMA := "1"

alias b:=build

pre-commit *args:
    cargo dev generate-all
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test {{args}}
    uvx pre-commit run --all-files --show-diff-on-failure

build:
    cargo build --release