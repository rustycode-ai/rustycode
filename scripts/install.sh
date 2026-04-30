#!/bin/bash
set -e

echo "Installing RustyCode..."

# Check for Rust/Cargo
if ! command -v cargo &> /dev/null; then
    echo "Error: cargo is not installed. Please install Rust first: https://rustup.rs/"
    exit 1
fi

# Clone and install
mkdir -p ~/.rustycode-build
cd ~/.rustycode-build
if [ -d "rustycode" ]; then
    cd rustycode
    git pull origin main
else
    git clone https://github.com/luengnat/rustycode.git
    cd rustycode
fi

echo "Building RustyCode..."
cargo build --release --package rustycode-cli

# Install to local bin
mkdir -p ~/.local/bin
cp target/release/rustycode ~/.local/bin/
echo "Done! RustyCode installed to ~/.local/bin/rustycode"
