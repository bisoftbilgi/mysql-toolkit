use linemux::MuxedLines;
use sqlx::postgres::PgPoolOptions;
use serde_json::Value;
use std::env;
use dotenv::dotenv;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL")?;
    let log_path = env::var("MONGODB_LOG_PATH")?;

    // 1. Postgres Bağlantısı
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    // 2. Tablo Oluşturma
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS mongodb_connection_logs (
            id SERIAL PRIMARY KEY,
            event_time TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
            client_ip TEXT,
            user_name TEXT,
            action TEXT
        )"
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS mongodb_audit_logs (
            id SERIAL PRIMARY KEY,
            event_time TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
            db_name TEXT,
            collection_name TEXT,
            command TEXT,
            duration_ms INTEGER
        )"
    )
    .execute(&pool)
    .await?;

    println!("🚀 MongoDB Logger is active. Watching: {}", log_path);

    // 3. Log Dosyasını İzleme
    let mut lines = MuxedLines::new()?;
    lines.add_file(&log_path).await?;

    while let Ok(Some(line)) = lines.next_line().await {
        let json_line: Value = match serde_json::from_str(line.line()) {
            Ok(v) => v,
            Err(_) => continue, // JSON değilse atla
        };

        // MongoDB Log Ayrıştırma Mantığı
        let component = json_line["c"].as_str().unwrap_or("");
        
        if component == "ACCESS" || component == "NETWORK" {
            // Bağlantı Logu
            let client_ip = json_line["remote"].as_str().unwrap_or("unknown");
            let action = json_line["msg"].as_str().unwrap_or("");
            
            sqlx::query("INSERT INTO mongodb_connection_logs (client_ip, action) VALUES ($1, $2)")
                .bind(client_ip)
                .bind(action)
                .execute(&pool)
                .await?;
        } else if component == "COMMAND" || component == "WRITE" {
            // Audit/Sorgu Logu
            let db_coll = json_line["attr"]["ns"].as_str().unwrap_or("");
            let command = json_line["attr"]["command"].to_string();
            let duration = json_line["attr"]["durationMillis"].as_i64().unwrap_or(0);

            sqlx::query("INSERT INTO mongodb_audit_logs (db_name, command, duration_ms) VALUES ($1, $2, $3)")
                .bind(db_coll)
                .bind(command)
                .bind(duration as i32)
                .execute(&pool)
                .await?;
        }
    }

    Ok(())
}
