# syntax=docker/dockerfile:1
FROM rust:1.98.1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml build.rs ./
COPY src ./src
COPY vendor ./vendor
COPY .cargo ./.cargo
ARG BRACEL_FEATURES=""
ARG TARGETARCH
# An imported source snapshot can be older than artifacts in the target cache.
# Refresh application inputs while retaining compiled vendored dependencies.
RUN --mount=type=cache,id=bracel-starter-target-${TARGETARCH},target=/app/target,sharing=locked \
    find src -type f -exec touch {} + \
    && touch Cargo.toml build.rs \
    && cargo build --frozen --release --bin bracel-starter --features "$BRACEL_FEATURES" \
    && mkdir -p /out && cp target/release/bracel-starter /out/bracel-starter

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 app && useradd --uid 10001 --gid app --no-create-home app
COPY --from=build /out/bracel-starter /usr/local/bin/bracel-starter
USER 10001:10001
ENV BIND_ADDR=0.0.0.0:3000 ENABLE_EXAMPLE=false
EXPOSE 3000
STOPSIGNAL SIGTERM
ENTRYPOINT ["bracel-starter"]
CMD ["serve"]
