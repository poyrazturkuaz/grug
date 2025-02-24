# List available recipes
default:
  @just --list

# Compile and install the Grug node software
install:
  cargo install --path bin

# Run tests
test:
  cargo test --all-targets

# Perform linting
lint:
  cargo +nightly clippy --bins --tests --benches --examples --all-features --all-targets

# Compile and optimize contracts (https://github.com/CosmWasm/rust-optimizer)
optimize:
  if [[ $(uname -m) =~ "arm64" ]]; then \
  docker run --rm -v "$(pwd)":/code \
    --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
    --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
    cosmwasm/optimizer-arm64:0.15.0; else \
  docker run --rm -v "$(pwd)":/code \
    --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
    --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
    --platform linux/amd64 \
    cosmwasm/optimizer:0.15.0; fi
