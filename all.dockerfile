# this dockerfile can build any of the cargo binary targets in this workspace
# just set BIN to the name of the binary you want to build

FROM rust:alpine as builder
RUN apk add --no-cache musl-dev

# build just dependencies first to cache them
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && touch src/lib.rs && cargo build --release && rm -r src

COPY . .
RUN cargo build --release --all-targets

FROM alpine
ARG BIN
COPY --from=builder /target/release/$BIN /usr/local/bin/entrypoint
CMD ["entrypoint"]
