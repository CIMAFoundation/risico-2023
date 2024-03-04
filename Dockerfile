# Use a base image with the latest version of Rust installed
FROM rust:latest

# install hdf5, netcdf, zlib
RUN apt-get update && apt-get install -y libhdf5-dev libnetcdf-dev zlib1g-dev

# Set the working directory in the container
WORKDIR /app

# Copy the local application code into the container
COPY . .


# Set the default command to run when a new container is started
CMD ["/bin/bash", "/app/build.sh"]




