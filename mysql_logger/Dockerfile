# 1. Aşama: Derleme (İnşaat)
# Rust'ın resmi alet çantasını indiriyoruz.
FROM rust:latest AS builder
WORKDIR /usr/src/mysql_logger
COPY . .
# Kodumuzu en yüksek hızda çalışacak şekilde (release) derliyoruz.
RUN cargo build --release --jobs 1

# 2. Aşama: Çalıştırma (Paketleme)
# Dev gibi Rust alet çantasını çöpe atıp, sadece programı çalıştıracak ufacık bir Linux alıyoruz.
FROM debian:bookworm-slim
# Veritabanına bağlanırken hata almamak için gerekli SSL sertifikalarını kuruyoruz.
RUN apt-get update && apt-get install -y libssl-dev ca-certificates tzdata && rm -rf /var/lib/apt/lists/*

WORKDIR /app
# İlk aşamada ürettiğimiz o tek parça motoru (mysql_logger) bu ufak Linux'un içine taşıyoruz.
COPY --from=builder /usr/src/mysql_logger/target/release/mysql_logger /app/mysql_logger
COPY config.toml /app/config.toml

# Konteyner ayağa kalktığında bu komutu çalıştır diyoruz.
CMD ["./mysql_logger"]
