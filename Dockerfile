# syntax=docker/dockerfile:1.4.0
FROM rust:1.85.0-slim-bookworm as builder

RUN apt-get update && apt-get install -y \
    libudev-dev \
    clang \
    pkg-config \
    libssl-dev \
    build-essential \
    cmake \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/* \
    && update-ca-certificates


ENV HOME=/home/root
WORKDIR $HOME/app

COPY . .

RUN cargo build --release  && cp target/release/kobe-* ./

FROM debian:bookworm-slim as cranker
RUN apt-get update && apt-get install -y libssl3 ca-certificates procps && rm -rf /var/lib/apt/lists/*
ENV APP="kobe-cranker"
WORKDIR /app
COPY --from=builder /home/root/app/${APP} ./
ENTRYPOINT ./$APP

FROM debian:bookworm-slim as api
RUN apt-get update && apt-get install -y libssl3 ca-certificates procps && rm -rf /var/lib/apt/lists/*
ENV APP="kobe-api"
WORKDIR /app
COPY --from=builder /home/root/app/${APP} ./
ENTRYPOINT ./$APP

FROM debian:bookworm-slim as writer-service
RUN apt-get update && apt-get install -y libssl3 ca-certificates procps && rm -rf /var/lib/apt/lists/*
ENV APP="kobe-writer-service"
WORKDIR /app
COPY --from=builder /home/root/app/${APP} ./
ENTRYPOINT ./$APP live

FROM debian:bookworm-slim as steward-writer-service
RUN apt-get update && apt-get install -y libssl3 ca-certificates procps && rm -rf /var/lib/apt/lists/*
ENV APP="kobe-steward-writer-service"
WORKDIR /app
COPY --from=builder /home/root/app/${APP} ./
ENTRYPOINT ./$APP listen
