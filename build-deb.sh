#!/bin/bash

function show_help() {
    echo "Usage: $0 [--help] [--build-image]"
    echo
    echo "Options:"
    echo "  --help          Show this help message and exit."
    echo "  --build-image   Build the Docker image required for building the Debian package."
}

build_image=false

for arg in "$@"; do
    case $arg in
        --help)
            show_help
            exit 0
            ;;
        --build-image)
            build_image=true
            ;;
        *)
            echo "Error: Unknown argument '$arg'"
            show_help
            exit 1
            ;;
    esac
done

if $build_image; then
    echo "Building the Docker image..."
    docker build -f deb-builder.Dockerfile -t system-mqtt-deb-builder:latest .
fi

echo "Building the Debian package..."
docker run --rm -v "$(pwd)":/app system-mqtt-deb-builder:latest cargo deb