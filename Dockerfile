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

ENTRYPOINT ["cargo", "build", "--release"]
