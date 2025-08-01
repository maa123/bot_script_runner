FROM nrt.vultrcr.com/docker.io/rust as rust

WORKDIR /app

COPY . .

RUN cargo build --release

FROM nrt.vultrcr.com/docker.io/golang:1.24 as golang

WORKDIR /app

COPY . .

RUN go build

FROM gcr.io/distroless/cc

WORKDIR /app

COPY --from=rust /app/target/release/bot_script_runner /app/target/release/

COPY --from=golang /app/bot_script_runner /app/bot_script_runner

CMD ["/app/bot_script_runner"]
