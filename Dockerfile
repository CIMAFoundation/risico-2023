# Use a base image with the latest version of Rust installed
FROM rust:latest

# install hdf5, netcdf, zlib
RUN apt-get update && apt-get install -y \
    curl libhdf5-dev libnetcdf-dev \
    build-essential cmake \
    zlib1g-dev

WORKDIR /app

# Copy the local application code into the container
COPY . .

ENV GIT_COMMIT_SHORT_HASH=43d6f36

ENTRYPOINT ["cargo", "build", "--release"]
