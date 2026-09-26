# syntax=docker/dockerfile:1
FROM rust:1.98.1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY starter/Cargo.toml starter/build.rs ./starter/
COPY starter/src ./starter/src
ARG BRACEL_FEATURES=""
ARG TARGETARCH
# An imported source snapshot can be older than artifacts in the target cache.
# Refresh local inputs so only immutable dependencies can survive this build step.
RUN --mount=type=cache,id=bracel-workspace-target-${TARGETARCH},target=/app/target,sharing=locked \
    --mount=type=cache,id=bracel-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    find crates starter/src -type f \( -name Cargo.toml -o -name build.rs -o -path '*/src/*' \) -exec touch {} + \
    && touch starter/Cargo.toml starter/build.rs \
    && cargo build --locked --release --bin bracel-starter --features "$BRACEL_FEATURES" \
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
