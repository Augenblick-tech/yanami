FROM debian:stable-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

COPY target/release/cmd /server/yanami
WORKDIR /server

ENTRYPOINT ["sh", "-c", "/server/yanami -c /config/config.toml"]
