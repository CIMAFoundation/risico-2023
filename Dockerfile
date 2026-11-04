# Ubuntu 16.04 keeps the produced binary compatible with old glibc versions.
# The release workflow builds a static musl binary instead (see
# .github/workflows/build-on-release.yml); this image is kept for building a
# glibc-linked binary locally with ./build-with-docker.sh.
FROM ubuntu:16.04

# NetCDF and HDF5 are pure Rust in this build; only a C toolchain (for the
# vendored zstd sources) and curl (for rustup) are needed.
RUN apt-get update && apt-get install -y \
    curl \
    build-essential

# Install the latest version of Rustup and the default stable toolchain
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /app

# Copy the local application code into the container
COPY . .

ENTRYPOINT ["cargo", "build", "--release", "--features=build-binary", "--target", "x86_64-unknown-linux-gnu"]
