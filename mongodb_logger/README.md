# MongoDB Audit & Connection Logger

A high-performance, resource-optimized log parser and forwarder built with Rust. This service continuously monitors MongoDB log files in real-time, parses the JSON-formatted logs, and securely routes authentication and audit events into a PostgreSQL database.

## 🚀 Key Features & Product Value
* **Resource Optimized (Rust):** Engineered to run seamlessly in highly constrained environments (e.g., 2GB RAM). 
* **Zero-Touch Setup:** Automatically creates necessary PostgreSQL tables (`mongodb_connection_logs` and `mongodb_audit_logs`) upon startup. No manual database migration required.
* **Separation of Concerns:** Intelligently splits logs into two distinct tables:
  * **Connection Logs:** Tracks `ACCESS` and `NETWORK` events (Client IP, authentication actions).
  * **Audit Logs:** Tracks `COMMAND` and `WRITE` operations (Database, collection, exact query, execution time).
* **Host Network Integration:** Bypasses Docker's internal networking (`network_mode: "host"`) to ensure zero-latency communication with strict, localhost-bound PostgreSQL instances.

## 📋 Prerequisites
1. **Docker & Docker Compose** installed on the host machine.
2. **PostgreSQL** running and accessible.
3. **MongoDB** configured to output JSON logs.
4. **MongoDB Profiling** enabled (Level 2 recommended for full query auditing):
   ```javascript
   db.setProfilingLevel(2, { slowms: 0 })
🛠️ Configuration
Create a .env file in the root directory with the following variables:

Kod snippet'i
# Use 127.0.0.1 since the container runs in host network mode
DATABASE_URL=postgres://<user>:<password>@127.0.0.1:5433/<database_name>
MONGODB_LOG_PATH=/var/log/mongodb/mongod.log
🚀 Deployment
Deploying the service is fully automated via Docker Compose.
(Note: If compiling on a low-resource machine, ensure sufficient Swap space is configured prior to building).

Bash
docker compose up -d --build
To view real-time log ingestion:

Bash
docker logs -f mongodb_logger_container
🗄️ Database Schema Details
The service maintains two primary tables:

mongodb_connection_logs

id (Serial, PK)

event_time (Timestamp)

client_ip (Text)

user_name (Text)

action (Text)

mongodb_audit_logs

id (Serial, PK)

event_time (Timestamp)

db_name (Text)

collection_name (Text)

command (Text)

duration_ms (Integer)

