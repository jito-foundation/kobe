# syntax=docker/dockerfile:1.4.0
FROM rust:1.75.0-slim-buster as builder


RUN apt-get update && apt-get install -y libudev-dev clang pkg-config libssl-dev build-essential cmake protobuf-compiler


RUN rustup component add rustfmt
RUN update-ca-certificates


ENV HOME=/home/root
WORKDIR $HOME/app

COPY . .

RUN --mount=type=cache,mode=0777,target=/home/root/app/target \
    --mount=type=cache,mode=0777,target=/usr/local/cargo/registry \
    RUST_BACKTRACE=1 cargo build --release  && cp target/release/kobe-* ./

FROM  debian:buster-slim as cranker
# need libssl1.1 for sentry, otherwise get this error:
# ./kobe-writer-service: error while loading shared libraries: libssl.so.1.1: cannot open shared object file: No such file or directory
RUN apt-get update && apt-get install -y libssl1.1 ca-certificates && rm -rf /var/lib/apt/lists/*
ENV APP="kobe-cranker"
WORKDIR /app
COPY --from=builder /home/root/app/${APP} ./
ENTRYPOINT ./$APP

FROM debian:buster-slim as api
# need libssl1.1 for sentry, otherwise get this error:
# ./kobe-writer-service: error while loading shared libraries: libssl.so.1.1: cannot open shared object file: No such file or directory
RUN apt-get update && apt-get install -y libssl1.1 ca-certificates && rm -rf /var/lib/apt/lists/*
ENV APP="kobe-api"
WORKDIR /app
COPY --from=builder /home/root/app/${APP} ./
ENTRYPOINT ./$APP

FROM debian:buster-slim as writer-service
# need libssl1.1 for sentry, otherwise get this error:
# ./kobe-writer-service: error while loading shared libraries: libssl.so.1.1: cannot open shared object file: No such file or directory
RUN apt-get update && apt-get install -y libssl1.1 ca-certificates && rm -rf /var/lib/apt/lists/*
ENV APP="kobe-writer-service"
WORKDIR /app
COPY --from=builder /home/root/app/${APP} ./
ENTRYPOINT ./$APP live

FROM debian:buster-slim as steward-writer-service
RUN apt-get update && apt-get install -y libssl1.1 ca-certificates && rm -rf /var/lib/apt/lists/*
ENV APP="kobe-steward-writer-service"
WORKDIR /app
COPY --from=builder /home/root/app/${APP} ./
ENTRYPOINT ./$APP listen
