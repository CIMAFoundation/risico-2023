#!/bin/bash
cargo build --target=x86_64-unknown-linux-musl
cp target/release/risico-2023 ./risico-2023