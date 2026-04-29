
FROM rust:1 as builder
WORKDIR /usr/src/mongodb_logger
COPY . .

RUN cargo build --release --jobs 1

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libpq-dev ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/mongodb_logger/target/release/mongodb_logger /usr/local/bin/mongodb_logger
CMD ["mongodb_logger"]
