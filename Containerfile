# FROM rust:latest AS builder

# COPY . /build/

# WORKDIR /build

# RUN apt-get update && apt-get install -y cmake

# RUN cargo clean
# RUN cargo build --release

FROM debian:stable-slim

# COPY --from=builder /build/target/release/yanami /server/
COPY target/release/yanami /server/
WORKDIR /server

ENTRYPOINT ["sh", "-c", "echo \"RUST_LOG variable is: $RUST_LOG\" && /server/yanami -c /config/config.toml"]
