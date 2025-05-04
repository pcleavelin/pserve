#!/bin/bash

cargo build --target wasm32-unknown-unknown --lib
cargo build --bin hello_server
