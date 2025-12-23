# Use Ubuntu 16.04 as the base image
FROM ubuntu:16.04

# install hdf5, netcdf, zlib
RUN apt-get update && apt-get install -y \
    libc6-dev \
    curl libhdf5-dev\ 
    libnetcdf-dev \
    build-essential\
    cmake \
    zlib1g-dev

# Install the latest version of Rustup and the default stable toolchain
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# install anaconda
RUN curl https://repo.anaconda.com/miniconda/Miniconda3-py38_4.12.0-Linux-x86_64.sh -o miniconda.sh
RUN bash miniconda.sh -b -p /root/miniconda

ENV PATH="/root/miniconda/bin:${PATH}"

# install conda packages
RUN conda install cmake 

WORKDIR /app

# Copy the local application code into the container
COPY . .

#ENV RUSTFLAGS="-C target-feature=+crt-static"
ENTRYPOINT ["cargo", "build", "--release", "--features=build-binary, static_deps", "--target", "x86_64-unknown-linux-gnu"]
