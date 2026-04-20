use config::{Config, File as ConfigFile};
use serde::Deserialize;
use sqlx::mysql::MySqlPoolOptions;
use sqlx::postgres::PgPoolOptions;
use sqlx::{MySqlPool, PgPool};
use std::collections::HashSet;
use std::fs;
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::fs as afs;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

const SINCEDB_DIR: &str = "./sincedb/";
const OUTPUT_DIR: &str = "./output/";
const LOG_PATTERN: &str = "/var/log/mysql/audit.log";

#[derive(Deserialize, Debug)]
struct AuditLogWrapper {
    audit_record: AuditRecord,
}

#[derive(Deserialize, Debug)]
struct AuditRecord {
    name: String,
    timestamp: String,
    user: String,
    host: String,
    ip: String,
    db: String,
    sqltext: Option<String>,
}

#[derive(Clone, Copy)]
enum OutputMode {
    Postgresql,
    Mysql,
}

#[derive(Deserialize)]
struct RawAppConfig {
    output_mode: String,
    db_url: Option<String>,
    db_table: Option<String>,
    hostname: Option<String>,
    ip: Option<String>,
    flush_interval_secs: Option<u64>,
    batch_size: Option<usize>,
}

struct AppConfig {
    mode: OutputMode,
    db_url: Option<String>,
    db_table: String,
    hostname: String,
    ip: String,
    flush_interval_secs: u64,
    batch_size: usize,
}

#[derive(Clone)]
struct LogEntry {
    timestamp: String,
    user: String,
    db: String,
    hostname: String,
    ip: String,
    message: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(SINCEDB_DIR)?;
    fs::create_dir_all(OUTPUT_DIR)?;

    let app_config = load_config()?;
    let output_mode = app_config.mode;
    let flush_interval_secs = app_config.flush_interval_secs;
    let batch_size = app_config.batch_size;

    let postgres_pool = match output_mode {
        OutputMode::Postgresql => {
            let db_url = app_config.db_url.as_deref().ok_or("db_url zorunlu")?;
            let pool = PgPoolOptions::new().max_connections(5).connect(db_url).await?;
            ensure_postgres_table(&pool, &app_config.db_table).await?;
            Some(pool)
        }
        _ => None,
    };
    
    let mysql_pool = match output_mode {
        OutputMode::Mysql => {
            let db_url = app_config.db_url.as_deref().ok_or("db_url zorunlu")?;
            let pool = MySqlPoolOptions::new().max_connections(5).connect(db_url).await?;
            ensure_mysql_table(&pool, &app_config.db_table).await?;
            Some(pool)
        }
        _ => None,
    };

    let db_table = app_config.db_table.clone();
    let config_hostname = app_config.hostname.clone();
    let config_ip = app_config.ip.clone();

    let (tx, mut rx) = mpsc::channel::<LogEntry>(10000);

    // Consumer
    tokio::spawn(async move {
        let mut buffer: Vec<LogEntry> = Vec::new();
        let mut last_flush = std::time::Instant::now();

        loop {
            let received = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
            if let Ok(Some(entry)) = received {
                buffer.push(entry);
            }

            if !buffer.is_empty()
                && (last_flush.elapsed() >= Duration::from_secs(flush_interval_secs)
                    || buffer.len() >= batch_size)
            {
                match output_mode {
                    OutputMode::Postgresql => {
                        if let Some(pool) = &postgres_pool {
                            let _ = flush_postgres(pool, &db_table, &mut buffer).await;
                        }
                    }
                    OutputMode::Mysql => {
                        if let Some(pool) = &mysql_pool {
                            let _ = flush_mysql(pool, &db_table, &mut buffer).await;
                        }
                    }
                }
                last_flush = std::time::Instant::now();
            }
        }
    });

    // Producer (YENİ DÜZELTİLEN KISIM: glob yerine direkt dosya kontrolü)
    let tracked_files = Arc::new(Mutex::new(HashSet::<PathBuf>::new()));

    loop {
        let entry = PathBuf::from(LOG_PATTERN);
        
        // Sadece dosya gerçekten varsa işlemi başlat
        if entry.exists() {
            let mut tracked = tracked_files.lock().unwrap();
            if !tracked.contains(&entry) {
                println!("MySQL Audit Log Kesfedildi: {:?}", entry);
                tracked.insert(entry.clone());
                let tx_c = tx.clone();
                let path_c = entry.clone();
                let host_c = config_hostname.clone();
                let ip_c = config_ip.clone();
                
                tokio::spawn(async move {
                    let _ = tail_file(path_c, tx_c, host_c, ip_c).await;
                });
            }
        }
        sleep(Duration::from_secs(10)).await;
    }
}

fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> {
    let raw: RawAppConfig = Config::builder()
        .add_source(ConfigFile::with_name("config"))
        .build()?
        .try_deserialize()?;

    let mode = match raw.output_mode.to_lowercase().as_str() {
        "postgresql" | "postgres" => OutputMode::Postgresql,
        "mysql" | "mariadb" => OutputMode::Mysql,
        other => return Err(format!("Gecersiz output_mode: {other}").into()),
    };

    let db_table = raw.db_table.unwrap_or_else(|| "mysql_audit_logs".to_string());
    
    Ok(AppConfig {
        mode,
        db_url: raw.db_url,
        db_table,
        hostname: raw.hostname.unwrap_or_else(|| "unknown-host".to_string()),
        ip: raw.ip.unwrap_or_else(|| "0.0.0.0".to_string()),
        flush_interval_secs: raw.flush_interval_secs.unwrap_or(10),
        batch_size: raw.batch_size.unwrap_or(1000),
    })
}

async fn ensure_postgres_table(pool: &PgPool, _table_name: &str) -> Result<(), sqlx::Error> {
    // 1. Connection Logs Tablosu
    let conn_stmt = "
        CREATE TABLE IF NOT EXISTS connection_logs (
            id BIGSERIAL PRIMARY KEY,
            log_time TIMESTAMPTZ NOT NULL,
            username TEXT,
            database_name TEXT,
            client_ip TEXT,
            action TEXT,
            cluster_name TEXT,
            server_name TEXT,
            server_ip TEXT,
            application_name TEXT
        )";
    sqlx::query(conn_stmt).execute(pool).await?;

    // 2. Audit Logs Tablosu (Sorgular)
    let audit_stmt = "
        CREATE TABLE IF NOT EXISTS audit_logs (
            id BIGSERIAL PRIMARY KEY,
            log_time TIMESTAMPTZ NOT NULL,
            username TEXT,
            database_name TEXT,
            session_id TEXT,
            statement_id TEXT,
            audit_type TEXT,
            statement_text TEXT,
            command TEXT,
            object_type TEXT,
            object_name TEXT,
            cluster_name TEXT,
            server_name TEXT,
            server_ip TEXT,
            client_ip TEXT,
            application_name TEXT
        )";
    sqlx::query(audit_stmt).execute(pool).await?;
    
    Ok(())
}

async fn flush_postgres(pool: &PgPool, _table_name: &str, entries: &mut Vec<LogEntry>) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    
    let conn_insert = "INSERT INTO connection_logs (log_time, username, database_name, client_ip, action, server_name) VALUES ($1::timestamptz, $2, $3, $4, $5, $6)";
    let audit_insert = "INSERT INTO audit_logs (log_time, username, database_name, client_ip, statement_text, audit_type, server_name) VALUES ($1::timestamptz, $2, $3, $4, $5, $6, $7)";

    for entry in entries.iter() {
        // Log "Connect" veya "Quit" ise Connection tablosuna, değilse Audit tablosuna!
        if entry.message == "[Connect]" || entry.message == "[Quit]" {
            let action = entry.message.replace("[", "").replace("]", "");
            sqlx::query(conn_insert)
                .bind(&entry.timestamp).bind(&entry.user).bind(&entry.db).bind(&entry.ip).bind(&action).bind(&entry.hostname)
                .execute(&mut *tx).await?;
        } else {
            sqlx::query(audit_insert)
                .bind(&entry.timestamp).bind(&entry.user).bind(&entry.db).bind(&entry.ip).bind(&entry.message).bind("Query").bind(&entry.hostname)
                .execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    println!("PostgreSQL'e yazildi (Split Mode): {} kayit", entries.len());
    entries.clear();
    Ok(())
}

async fn ensure_mysql_table(pool: &MySqlPool, table_name: &str) -> Result<(), sqlx::Error> {
    let stmt = format!("CREATE TABLE IF NOT EXISTS {table_name} (id BIGINT AUTO_INCREMENT PRIMARY KEY, log_timestamp varchar(100), user_name varchar(100), database_name varchar(100), hostname varchar(100), ip_address varchar(100), message varchar(1000), inserted_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)");
    sqlx::query(&stmt).execute(pool).await?;
    Ok(())
}

async fn flush_mysql(pool: &MySqlPool, table_name: &str, entries: &mut Vec<LogEntry>) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let insert_stmt = format!("INSERT INTO {table_name} (log_timestamp, user_name, database_name, hostname, ip_address, message) VALUES (?, ?, ?, ?, ?, ?)");
    for entry in entries.iter() {
        sqlx::query(&insert_stmt).bind(&entry.timestamp).bind(&entry.user).bind(&entry.db).bind(&entry.hostname).bind(&entry.ip).bind(&entry.message).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    println!("MySQL'e yazildi: {} kayit", entries.len());
    entries.clear();
    Ok(())
}

async fn tail_file(
    path: PathBuf,
    tx: mpsc::Sender<LogEntry>,
    config_hostname: String,
    config_ip: String,
) -> tokio::io::Result<()> {
    let file_name = path.file_name().unwrap().to_str().unwrap();
    let sincedb_path = format!("{}{}.offset", SINCEDB_DIR, file_name);

    let mut current_offset = fs::read_to_string(&sincedb_path)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);

    let file = afs::File::open(&path).await?;
    let mut reader = BufReader::new(file);

    let meta = afs::metadata(&path).await?;
    if meta.len() < current_offset {
        current_offset = 0;
    }
    reader.seek(SeekFrom::Start(current_offset)).await?;

    let mut line = String::new();

    loop {
        line.clear();
        let len = reader.read_line(&mut line).await?;

        if len == 0 {
            sleep(Duration::from_millis(500)).await;
            continue;
        }

        let trimmed = line.trim();
        
        if trimmed.is_empty() {
            current_offset += len as u64;
            continue;
        }

        match serde_json::from_str::<AuditLogWrapper>(trimmed) {
            Ok(wrapper) => {
                let rec = wrapper.audit_record;
                let message = rec.sqltext.unwrap_or_else(|| format!("[{}]", rec.name));

                let _ = tx.send(LogEntry {
                    timestamp: rec.timestamp,
                    user: rec.user,
                    db: rec.db,
                    hostname: if rec.host.is_empty() { config_hostname.clone() } else { rec.host },
                    ip: if rec.ip.is_empty() { config_ip.clone() } else { rec.ip },
                    message,
                }).await;
            }
            Err(e) => {
                eprintln!("JSON Parse Hatasi: {} | Satir: {}", e, trimmed);
            }
        }

        current_offset += len as u64;
        let _ = afs::write(&sincedb_path, current_offset.to_string()).await;
    }
}
