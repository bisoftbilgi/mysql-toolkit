# MySQL Logger & Audit Toolkit

**Enterprise-grade, Dockerized Rust service for processing MySQL audit logs and routing them to PostgreSQL.** Built with Rust, optimized for low-resource environments (2GB RAM), and designed for high-granularity security monitoring.

---

## 1. Why This Project?

Standard MySQL logs are often hard to parse and decentralised. This toolkit:
* **Centralises Security:** Streams logs from MySQL to a central PostgreSQL instance.
* **Split-Table Logic:** Automatically separates connection events (`Connect/Quit`) from data operations (`Query/Table Access`).
* **Dockerized Deployment:** Zero-dependency installation on the host system.
* **Resource Optimized:** Specifically tuned to compile and run on low-end servers (2GB RAM) without crashing.

---

## 2. Architecture Overview

\`\`\`text
[ MySQL 8.0 ] --(Percona Audit)--> [ audit.log (JSON) ]
                                          |
                                          v
[ Docker: mysql_logger (Rust) ] <---(File Stream)
            |
            +---(Logic: Split Mode)
            |
            v
[ PostgreSQL 15-16 ] ---> [ connection_logs ] & [ audit_logs ]
\`\`\`

---

## 3. Requirements

### MySQL Source
* **Version:** MySQL 8.0.x (Tested on Rocky Linux / RHEL environments).
* **Plugin:** Percona Audit Log Plugin (Mandatory for JSON output).

### Host System
* **Docker & Docker Compose**
* **Memory:** Minimum 2GB RAM (Compilation requires specific flags).

### Destination DB
* **PostgreSQL 15 or 16** (Target tables are automatically created).

---

## 4. MySQL & Percona Configuration

To enable the audit stream, your MySQL configuration (`my.cnf` or `audit_log.cnf`) must include:

\`\`\`ini
[mysqld]
# Percona Audit Plugin Config
audit_log_format=JSON
audit_log_handler=FILE
audit_log_policy=ALL
audit_log_rotate_on_size=100M
audit_log_rotations=5
\`\`\`

---

## 5. Deployment (Quick Start)

### Environment Setup
Create a \`.env\` file in the root directory:
\`\`\`bash
DATABASE_URL=postgres://user:password@host:5433/postgres
LOG_PATH=/var/log/mysql/audit.log
\`\`\`

### Resource-Aware Build
Due to the 2GB RAM limitation on production servers, the build is restricted to a single job to prevent OOM (Out of Memory) kills:

\`\`\`bash
# Docker Compose will use the optimized Dockerfile
docker compose up -d --build
\`\`\`

*Note: The Dockerfile uses \`cargo build --release --jobs 1\` to ensure stability during compilation.*

---

## 6. PostgreSQL Schema (Auto-Generated)

The service maintains two primary tables in PostgreSQL:

| Table | Purpose | Key Columns |
| :--- | :--- | :--- |
| \`connection_logs\` | Tracking sessions | \`username\`, \`client_ip\`, \`action\` (Connect/Quit) |
| \`audit_logs\` | Tracking data changes | \`statement_text\`, \`db_name\`, \`audit_type\` |

---

## 7. MongoDB Integration (Monitoring)

The toolkit also supports MongoDB monitoring via **Slow Query Logging**.

### Enable MongoDB Logging (0ms Threshold)
To log every operation in MongoDB:
1.  Set \`operationProfiling.mode: slowOp\`
2.  Set \`slowOpThresholdMs: 0\`

Logs will stream to \`/var/log/mongodb/mongod.log\` in JSON format, compatible with this toolkit's future extensions.

---

## 8. Troubleshooting & Logs

**Check if the logger is alive:**
\`\`\`bash
docker logs -f mysql_logger_service
\`\`\`

**Common Issues:**
* **Permission Denied:** Ensure the Docker user has read access to \`/var/log/mysql/\`.
* **Database Connection:** Verify that the PostgreSQL port (default 5433) is reachable from inside the container.
* **Log Format:** If logs aren't appearing, verify \`audit_log_format=JSON\` is set in MySQL.

---

## 9. Development

To build manually without Docker:
\`\`\`bash
cargo build --release --jobs 1
\`\`\`
