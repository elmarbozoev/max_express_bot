FROM rust:1.77-slim as builder

WORKDIR /app

COPY . .

RUN cargo build --release

ENTRYPOINT [ "./target/release/max_express_bot" ]