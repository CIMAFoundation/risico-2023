#!/bin/bash
docker build -t risico-2023 .
docker run --rm -v $(pwd):/app risico-2023
# copy the binary to the host
cp risico-2023:/app/target/release/risico-2023 ./risico-2023
