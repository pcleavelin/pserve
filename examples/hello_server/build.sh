#!/bin/bash

cargo build --target wasm32-unknown-unknown --release --lib
cargo build --bin hello_server --release
