# Use a base image with the latest version of Rust installed
FROM rust:latest



# install hdf5, netcdf, zlib
RUN apt-get update && apt-get install -y curl musl-tools musl-dev \
    build-essential \
    zlib1g-dev

WORKDIR /tmp
RUN wget https://github.com/HDFGroup/hdf5/archive/refs/tags/hdf5-1_12_3.tar.gz \
    && tar -xzf hdf5-1_12_3.tar.gz

RUN wget https://github.com/Unidata/netcdf-c/archive/refs/tags/v4.9.2.tar.gz \
    && tar -xzf v4.9.2.tar.gz

WORKDIR /tmp
# Download, build, and install HDF5 statically
RUN cd hdf5-hdf5-1_12_3 \
    && CC=musl-gcc ./configure --prefix=/usr/local --enable-static --disable-shared \
    && make -j$(nproc) \
    && make install

WORKDIR /tmp/netcdf-c-4.9.2

# Commented out to try to build interactively

# RUN CC=musl-gcc CFLAGS="-std=gnu99" CPPFLAGS="-I/usr/local/include" LDFLAGS="-L/usr/local/lib -L/usr/lib/x86_64-linux-gnu" ./configure --prefix=/usr/local --enable-static --disable-shared
# RUN make -j$(nproc) \
#     && make install

# # Set environment variables to use static linking
# ENV HDF5_VERSION=1.12.3
# ENV NETCDF4_DIR=/usr/local
# ENV HDF5_DIR=/usr/local
# ENV LD_LIBRARY_PATH=/usr/local/lib
# ENV RUSTFLAGS="-C link-args=-Wl,-rpath,/usr/local/lib"

# # Add the musl target
# RUN rustup target add x86_64-unknown-linux-musl

# # Set the working directory in the container
# WORKDIR /app

# # Copy the local application code into the container
# COPY . .


# # Set the default command to run when a new container is started
# # CMD ["/bin/bash", "/app/build.sh"]
# ENTRYPOINT ["/bin/bash"]
