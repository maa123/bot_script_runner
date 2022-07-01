FROM rust:alpine as rust

WORKDIR /app

RUN apk --no-cache --update add \
    python3

COPY . .

RUN cargo build --release

FROM golang:1.18-alpine as golang

WORKDIR /app

COPY . .

RUN go build

FROM alpine

WORKDIR /app

COPY --from=rust /app/target/release/bot_script_runner /app/target/release/

COPY --from=golang /app/bot_script_runner /app/bot_script_runner

CMD ["/app/bot_script_runner"]