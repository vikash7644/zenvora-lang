FROM rust:1-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin zv

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/zv /usr/local/bin/zv
ENV ZENVORA_API_HOST=0.0.0.0
EXPOSE 8787
CMD ["sh", "-c", "zv --compiler-api ${PORT:-8787}"]
