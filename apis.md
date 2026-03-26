# MCCP Web Console — Required APIs

This document defines the REST + WebSocket APIs needed by the **MCCP Web Console** (`web/`) to manage/configure MCCP and to receive realtime updates.

Notes:
- **No authentication is required** for the web console (single-user mode). If you later enable agent tokens, you can optionally accept `Authorization: Bearer <token>`.
- Current server already exposes some endpoints (see **Existing endpoints**). Most management/config APIs below are **not implemented yet**.

---

## Conventions

### Base URL
- HTTP: `http://<host>:<port>` (default today: `http://localhost:7422`)
- WebSocket: `ws(s)://<host>:<port>/ws`

### Content types
- Request JSON: `Content-Type: application/json`
- Response JSON: `application/json`

### Error shape (recommended)
For new endpoints, prefer a consistent envelope:
```json
{ "ok": false, "err": "human readable message", "data": null }
```
Or plain non-2xx responses with a text body are acceptable, but the web client works best with structured JSON.

### IDs
- `project_id`: stable string ID (e.g., hash of root path) — must not change between restarts
- `task_id`: stable unique string (uuid recommended)

### Timestamps
- Use RFC3339 / ISO8601 strings (UTC): `2026-03-26T11:26:59Z`

---

## Existing endpoints (already in repo)

These exist in `crates/mccp-server/src/server.rs`:

### `GET /health`
**Purpose:** health/version.

**Response**
```json
{ "status": "ok", "version": "0.1.0" }
```

### `GET /index/status`
**Purpose:** current indexing status snapshot (currently only for the in-memory/default project).

**Response** (`IndexingStatus` from `mccp-indexer`)
```json
{
  "project_id": "...",
  "file_count": 123,
  "indexed_files": 120,
  "queue_depth": 0,
  "is_watching": true
}
```

### `GET /index/progress` (SSE)
**Purpose:** streaming indexing progress updates.

**Event data** (`IndexProgress`)
```json
{
  "phase": "chunking",
  "current": 10,
  "total": 100,
  "percentage": 10,
  "project_id": "..."
}
```

### `GET /projects`
**Purpose:** list projects.

**Status:** currently returns an empty list; real project management should move to `/api/v1/projects` below.

---

## Required REST endpoints (management/config)

### 1) System status

#### `GET /api/v1/system/status`
**Purpose:** show uptime + version + basic runtime info for the dashboard.

**Response**
```json
{
  "version": "0.1.0",
  "started_at": "2026-03-26T10:00:00Z",
  "uptime_ms": 5123456,
  "pid": 12345,
  "http_addr": "127.0.0.1:7422"
}
```

**Side effects:** none.

#### `GET /api/v1/system/metrics`
**Purpose:** CPU/RAM stats for the dashboard.

**Response**
```json
{
  "cpu_percent": 12.5,
  "ram_used_bytes": 123456789,
  "ram_total_bytes": 34359738368,
  "load_avg_1": 0.42
}
```

**Side effects:** none.

---

### 2) Configuration management

#### `GET /api/v1/config`
**Purpose:** fetch current effective config (for editing in the UI).

**Response**
```json
{
  "path": "/home/user/.mccp/config.toml",
  "toml": "[daemon]\nhttp_port = 7422\n...\n",
  "parsed": {
    "daemon": { "http_port": 7422 },
    "indexer": { "watch_enabled": true }
  }
}
```
- `parsed` is optional (but helpful). If omitted, return `{ path, toml }`.

**Side effects:** none.

#### `PUT /api/v1/config`
**Purpose:** apply a new system config.

**Request**
```json
{ "toml": "[daemon]\nhttp_port = 7422\n...\n" }
```

**Response**
```json
{ "ok": true }
```

**Side effects / behavior:**
- Validate TOML and return `400` with details if invalid.
- Write to config file (e.g., `~/.mccp/config.toml`).
- For settings that require restart (e.g., HTTP port), either:
  - return a field like `restart_required: true`, or
  - hot-apply where safe.

---

### 3) Project management

#### `GET /api/v1/projects`
**Purpose:** list known projects.

**Response**
```json
[
  {
    "id": "proj_x",
    "name": "my-repo",
    "root_path": "/path/to/repo",
    "status": "indexed",
    "last_indexed_at": "2026-03-26T10:30:00Z",
    "file_count": 1200,
    "chunk_count": 5400
  }
]
```

**Side effects:** none.

#### `POST /api/v1/projects`
**Purpose:** add/register a new project.

**Request**
```json
{
  "name": "my-repo",
  "root_path": "/path/to/repo",
  "watch": true
}
```

**Response**
```json
{ "id": "proj_x" }
```

**Side effects:**
- Persist project registry.
- Optionally enqueue an initial index task (if `index_immediately=true`).

#### `PATCH /api/v1/projects/{project_id}`
**Purpose:** update a project’s mutable settings (name, watch, ignore patterns, etc.).

**Request** (example)
```json
{ "name": "new-name", "watch": false }
```

**Side effects:**
- Update persisted registry.
- If `watch` toggles, start/stop watcher for that project.

#### `DELETE /api/v1/projects/{project_id}`
**Purpose:** remove a project.

**Side effects:**
- Stop watchers and cancel running tasks for the project (or refuse if busy).
- Optionally delete stored index data for the project.

#### `POST /api/v1/projects/{project_id}/reindex`
**Purpose:** force a full reindex.

**Request**
```json
{ "force": true }
```

**Response**
```json
{ "task_id": "task_abc" }
```

**Side effects:**
- Clears file-hash cache / index state.
- Enqueues an indexing task and emits progress updates.

---

### 4) Task management

#### `GET /api/v1/tasks?state=active|finished|all&project_id=...`
**Purpose:** list tasks (for Tasks page + dashboard counters).

**Response**
```json
[
  {
    "id": "task_abc",
    "kind": "reindex",
    "project_id": "proj_x",
    "state": "running",
    "title": "Reindex proj_x",
    "created_at": "2026-03-26T10:31:00Z",
    "started_at": "2026-03-26T10:31:02Z",
    "progress": { "current": 10, "total": 100, "percentage": 10, "phase": "chunking" }
  }
]
```

**Side effects:** none.

#### `POST /api/v1/tasks/{task_id}/cancel`
**Purpose:** cancel a running/queued task.

**Response**
```json
{ "ok": true }
```

**Side effects:**
- Stops work as soon as cancellation is observed.
- Emits `tasks.updated` over WS.

---

## Required WebSocket API (realtime updates)

### `GET /ws` (WebSocket upgrade)
**Purpose:** push realtime updates to the web UI without polling.

#### Message envelope
All server-to-client messages use a single JSON envelope:
```json
{
  "type": "system.metrics",
  "ts": "2026-03-26T11:26:59Z",
  "data": { }
}
```

#### Client → server messages

##### Subscribe (optional)
The client will send this on connect (server may ignore and just broadcast everything):
```json
{ "type": "subscribe", "topics": ["system", "metrics", "index", "tasks", "projects"] }
```

##### Ping (optional)
```json
{ "type": "ping" }
```
Server can reply:
```json
{ "type": "pong", "ts": "...", "data": {} }
```

---

### Server → client event types

#### `system.status`
Payload: `SystemStatus` (same as REST `/api/v1/system/status`).

When sent:
- immediately on connect
- periodically (e.g., every 5–10s)

#### `system.metrics`
Payload: `SystemMetrics` (same as REST `/api/v1/system/metrics`).

When sent:
- periodically (e.g., every 1s–5s)

#### `index.progress`
Payload: `IndexProgress`.

When sent:
- during indexing/reindexing

#### `tasks.snapshot`
Payload: `TaskInfo[]`.

When sent:
- immediately on connect
- when a client subscribes

#### `tasks.updated`
Payload: `TaskInfo`.

When sent:
- on every task lifecycle change
- on progress updates

#### `projects.snapshot`
Payload: `ProjectInfo[]`.

When sent:
- immediately on connect

#### `projects.updated`
Payload: `ProjectInfo`.

When sent:
- when a project is created/updated/deleted or its status changes

---

## Logs APIs

### REST: `GET /api/v1/logs`
**Purpose:** fetch historical logs for the Logs page, with server-side filtering/pagination.

**Query params**
- `level` (optional): `TRACE|DEBUG|INFO|WARN|ERROR`
- `q` (optional): substring match against message/target/span
- `target` (optional): prefix or exact match (e.g., `mccp_server`)
- `since` (optional): RFC3339 timestamp (return logs at/after)
- `until` (optional): RFC3339 timestamp
- `limit` (optional, default 500, max 5000)
- `cursor` (optional): opaque cursor for pagination

**Response**
```json
{
  "items": [
    {
      "id": "log_...",
      "ts": "2026-03-26T11:26:59Z",
      "level": "INFO",
      "target": "mccp_server",
      "span": "request",
      "message": "HTTP server listening on 127.0.0.1:7422"
    }
  ],
  "next_cursor": "..."
}
```

**Side effects:** none.

### WS events

#### `logs.snapshot`
Payload: `LogLine[]` (recent N lines). Sent immediately on connect.

#### `logs.line`
Payload: `LogLine`. Sent for every new log line.

Recommended behavior:
- Keep an in-memory ring buffer (e.g., last 2k–10k lines).
- Also persist to disk for history used by REST `GET /api/v1/logs`.

---

## Why WS + REST?
- REST is used for **commands** (create project, reindex, cancel task, apply config).
- WS is used for **status** (metrics, uptime, progress, live task updates).

The current Rust server already has SSE for index progress; WS will unify all realtime streams into a single channel.
