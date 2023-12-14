FROM rust:1.74-alpine as builder
WORKDIR /usr/src/app

RUN apk add --no-cache musl-dev protobuf-dev

# build just dependencies first to cache them
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && touch src/lib.rs && cargo build --release && rm -r src

COPY . .
RUN cargo build --release --all-targets

FROM alpine
ARG BIN
COPY --from=builder /usr/src/app/target/release/$BIN /usr/local/bin/entrypoint
CMD ["entrypoint"]
