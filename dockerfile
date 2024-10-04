FROM rust:latest AS builder

COPY . /build/

WORKDIR /build

RUN apt-get update && apt-get install -y musl-tools
RUN rustup default nightly
RUN rustup target add x86_64-unknown-linux-musl

RUN cargo clean
RUN cargo build --target x86_64-unknown-linux-musl --release

FROM alpine:latest

RUN apk add tzdata && cp /usr/share/zoneinfo/Asia/Shanghai /etc/localtime \
    && echo "Asia/Shanghai" > /etc/timezone 

COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/yanami /server/

WORKDIR /server

ENTRYPOINT ["/server/yanami", "-c", "/config/config.toml"]
