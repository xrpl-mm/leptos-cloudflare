#!/bin/sh

wasm-pack build --target=web -- --features hydrate --no-default-features

# workers-rs only allows static files to be served from the pkg/ directory
# when deploying Workers Sites.
cp -f css/* pkg/
cp -f static/* pkg/

cargo install --git https://github.com/xrpl-mm/workers-rs --rev 3883bf7d5cb599a21b7c279607c29e307bb4ba2e 

worker-build --release -- --features "ssr console_error_panic_hook" --no-default-features --bin example
