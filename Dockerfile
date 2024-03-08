# Use a base image with the latest version of Rust installed
FROM rust:latest

# install hdf5, netcdf, zlib
RUN apt-get update && apt-get install -y \
    libc6-dev \
    curl libhdf5-dev\ 
    libnetcdf-dev \
    build-essential\
    cmake \
    zlib1g-dev

WORKDIR /app

# Copy the local application code into the container
COPY . .

ENV RUSTFLAGS="-C target-feature=+crt-static"
ENTRYPOINT ["cargo", "build", "--release", "--features", "static_deps", "--target", "x86_64-unknown-linux-gnu"]
