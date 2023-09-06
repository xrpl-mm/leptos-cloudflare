#!/bin/sh

set -ex

cargo watch --workdir . --why -i worker/ -i README.md -s "cd client && pwd && wasm-pack build --target=web"

set +ex
