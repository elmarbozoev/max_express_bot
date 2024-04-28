FROM rust:1.77-buster as builder

WORKDIR /app

ARG DATABASE_URL

ENV DATABASE_URL=$DATABASE_URL

COPY . .

RUN cargo build --release

FROM debian:buster-slim

WORKDIR /usr/local/bin

COPY --from=builder /app/target/release/max_express_bot .

CMD ["./max_express_bot"]