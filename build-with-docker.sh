#!/bin/bash
echo "Building and running the application with Docker"
echo "Building the Docker image"
docker build -t risico-2023 .
echo "Running the Docker container"
docker run --rm -v $(pwd):/app risico-2023

