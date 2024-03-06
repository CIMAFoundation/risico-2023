#!/bin/bash
docker build -t risico-2023 .
docker run --rm -v $(pwd):/app risico-2023

