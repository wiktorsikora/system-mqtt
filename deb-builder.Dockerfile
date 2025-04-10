# Use the official Rust image for building
FROM rust:1.86.0-bookworm AS builder
# Install dependencies and build the application
RUN apt-get update && apt-get install -y libdbus-1-dev clang libsensors-dev

RUN cargo install cargo-deb

WORKDIR /app