FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

# Switch to vscode user for Rust toolchain installation
# This ensures proper ownership of cargo/rustup directories
USER vscode

# Install nightly toolchain for Rust edition 2024
RUN rustup toolchain install nightly \
    && rustup default nightly \
    && rustup component add rustfmt clippy rust-analyzer

# Install additional useful tools
RUN cargo install cargo-watch cargo-nextest

# Switch back to root for any system-level operations
USER root

# Set working directory
WORKDIR /workspaces/mcpg
