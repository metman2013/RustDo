# RustDo Hub container. Build from this folder:  docker build -t rustdo-hub .
# Or use the published image: ghcr.io/metman2013/rustdo-hub (see docs/hub.md).

FROM rust:1-alpine AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

# Just the static binary (`--target binary --output type=local,dest=out`): the Linux
# downloads on the Releases page come from here.
FROM scratch AS binary
COPY --from=build /src/target/release/rustdo-hub /rustdo-hub

FROM alpine:3.20
# su-exec: the entrypoint starts as root only to make /data writable (Home Assistant and
# NAS bind mounts are often owned by root), then runs the hub as `rustdo`.
RUN apk add --no-cache su-exec \
    && adduser -D -H -u 1000 rustdo \
    && mkdir /data && chown rustdo:rustdo /data
COPY --from=build /src/target/release/rustdo-hub /usr/local/bin/rustdo-hub
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod 755 /usr/local/bin/docker-entrypoint.sh
ENV RUSTDO_HUB_DATA=/data \
    RUSTDO_HUB_PORT=7979
VOLUME /data
EXPOSE 7979
HEALTHCHECK --interval=1m --timeout=5s CMD wget -qO- http://127.0.0.1:7979/api/hello >/dev/null || exit 1
# Every change is saved straight away (and atomically), so stopping it is always safe.
# Run with `--init` (compose: `init: true`) so `docker stop` doesn't wait 10 seconds.
ENTRYPOINT ["docker-entrypoint.sh"]
