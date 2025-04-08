# Use the official Rust image for building
FROM rust:1.86.0-bullseye AS builder

# Set the working directory
WORKDIR /usr/src/system-mqtt

# Install dependencies and build the application
RUN apt-get update && apt-get install -y libdbus-1-dev clang libsensors-dev

# Copy the project files excluding the target directory
COPY Cargo.toml Cargo.lock ./
COPY src ./src


RUN cargo build --release

# Create a minimal runtime image
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y libdbus-1-3 libsensors5 && \
    rm -rf /var/lib/apt/lists/*

# Copy the compiled binary from the builder stage
COPY --from=builder /usr/src/system-mqtt/target/release/system-mqtt /usr/bin/system-mqtt

# Set the working directory
WORKDIR /app
# Copy the configuration file

# Expose the configuration file path
VOLUME ["/etc/system-mqtt.yaml"]

# Set the default command
ENTRYPOINT ["system-mqtt", "--config-file", "/app/config/system-mqtt.yaml", "run", "--log-to-stderr"]