# LeSearch — System Design

**Version**: 0.2 (review synthesis applied — A1–A12, D1–D5)
**Date**: 2026-04-14
**Status**: Pre-implementation, ruthless v0.1.0 scope locked

Reads with: `PRODUCT_SPEC.md` (vision), `PRD.md` (requirements), `EPICS_AND_STORIES.md` (backlog).

---

## 1. Architecture Overview

LeSearch v0.1.0 is a **single-process Rust daemon** with five concerns layered inside one binary. Phase B splits AVM into a separate sidecar and adds remote transports. Every layer has a single well-defined responsibility and a narrow interface.

```
┌─ L6 Clients ────────────────────────────────────────────────────────┐
│  Rust CLI  │  Tauri desktop  │  Responsive web (loopback)           │
│  (Phase B: native iOS / macOS apps)                                 │
└──────────────────────┬─────────────────────────────────────────────┘
                       │ WebSocket  JSON-RPC 2.0  + binary mux
                       │ + per-session bearer + Origin allowlist (FR-40)
┌──────────────────────▼────────────────────────────────────────────┐
│ L5 Transport                                                       │
│  v0.1.0:  direct (127.0.0.1 loopback ONLY)                         │
│  Phase B: ziti (Remote Access)  │  noise-ws (Relay)                │
└──────────────────────┬────────────────────────────────────────────┘
                       │
┌──────────────────────▼────────────────────────────────────────────┐
│ L4 LeSearch Daemon  (Rust, tokio + axum, single binary)           │
│                                                                    │
│   ┌ Protocol router ──┐  ┌ Agent manager ──┐  ┌ A2A gateway ──┐   │
│   │ JSON-RPC dispatch │  │ state machine    │  │ read-only card│   │
│   └───────────────────┘  └──────────────────┘  └───────────────┘   │
│   ┌ Keyring ──────────┐  ┌ Session writer ──┐  ┌ FTS indexer ─┐   │
│   │ Ed25519 + X25519  │  │ JCS-canonical +  │  │ SQLite FTS5   │   │
│   │                   │  │ Ed25519-signed + │  │ (substring    │   │
│   │                   │  │ hash-chained     │  │  search v0.1) │   │
│   │                   │  │ JSONL +          │  │               │   │
│   │                   │  │ segment manifests│  │               │   │
│   └───────────────────┘  └──────────────────┘  └───────────────┘   │
│   ┌ AVM policy (in-process trait, v0.1.0) ─────────────────────┐   │
│   │ trait PolicyEngine; per-provider enforcement_mode: strict  │   │
│   │ where interceptable (Claude Code) / audit-only (Codex)     │   │
│   │ Phase B may split this into a separate UDS sidecar.        │   │
│   └───────────────────────────────────────────────────────────┘   │
└──────────────────────┬─────────────────────────────────┬─────────┘
                       │                                  │
                     stdio                              StorageBackend trait
                     MCP / ACP                          (FR-10)
                       │                                  │
              ┌────────▼─────────┐              ┌─────────▼──────────┐
              │ L1 Providers     │              │ L2 Storage         │
              │ v0.1.0:          │              │ v0.1.0 default:    │
              │   Claude Code    │              │   PlainDirBackend  │
              │   Codex (audit-  │              │   (chmod 700 dirs) │
              │   only mode)     │              │ Phase B / opt-in:  │
              │ Phase B:         │              │   AgentFsBackend   │
              │   OpenCode       │              │   (--features      │
              │   Gemini         │              │    agentfs)        │
              │   generic-a2a    │              │ + session log      │
              └──────────────────┘              │ + FTS5 mirror      │
                                                └────────────────────┘
```

> **CGVirtualDisplay is NOT in this diagram.** It remains a research track (cmux salvage branch) — never on the v0.1.0 or v1.0 architecture.

## 2. Components

### 2.1 L4 — Daemon (Rust)

**Responsibility**: own agent state, route protocol messages, enforce policy via AVM, persist sessions, expose A2A.

**Crate layout — single monorepo `aryateja2106/lesearch`** (Cargo workspace, **5–7 crates only** for v0.1.0 per A2):

| Crate | Role | v0.1.0? |
|---|---|---|
| `lesearch-core` | Tokio runtime + axum + agent manager + PTY (`portable-pty` + `alacritty_terminal`) + keyring + in-process AVM trait. Top-level wiring. | ✅ |
| `lesearch-protocol` | JSON-RPC 2.0 types, message definitions, binary mux framing, `A2A-Version` negotiation. | ✅ |
| `lesearch-storage` | `StorageBackend` trait + `PlainDirBackend` (default) + `AgentFsBackend` (feature-gated) + session log writer (JCS + Ed25519 + hash chain + segment manifests) + FTS5 indexer + jsongrep adapter (Phase B). | ✅ |
| `lesearch-providers` | `AgentProvider` trait + per-provider modules (`claude`, `codex` in v0.1.0; `opencode`, `gemini`, `generic-a2a` in Phase B). One crate, multiple `mod`s — not one crate per provider. | ✅ |
| `lesearch-cli` | `clap`-based `lesearch` binary. | ✅ |
| `lesearch-web` | React + TanStack Router, served by daemon at `127.0.0.1:6767/`. Same code base as the Tauri inner webview. | ✅ |
| `lesearch-a2a` (optional 7th crate) | A2A 1.0.0 facade — read-only agent card + profile + version negotiation in v0.1.0; inbound dispatch + OpenFused inbox in Phase B. May be folded into `lesearch-core` if the surface stays thin. | ✅ (or merged) |

**Killed / deferred** (per A2 ruthless cut):

- `lesearch-daemon` + `lesearch-agent-manager` + `lesearch-pty` + `lesearch-keyring` → folded into `lesearch-core`
- `lesearch-avm-client` → in-process trait, no separate crate
- `lesearch-search` → folded into `lesearch-storage`
- `lesearch-providers-claude` / `-codex` / `-opencode` / `-gemini` / `-generic-a2a` → modules under `lesearch-providers`, not separate crates
- `lesearch-transport-ziti` + `lesearch-transport-noise` → Phase B; v0.1.0 transport is loopback only and lives inside `lesearch-core`

**Why monorepo + 5–7 crates**: solo maintainer; a 6-repo / 19-crate split is the wrong place to optimize at v0.1.0. Reference: Zed and GitButler both run as Rust monorepos.

**Runtime**: single `tokio` runtime on daemon; all async. No threads besides blocking work (FS I/O on block-in-place or spawn-blocking).

**Entry points**: `lesearch daemon start` (foreground) / system service (launchd/systemd). Single binary. Default home `$LESEARCH_HOME = ~/.lesearch`.

### 2.2 AVM Policy Engine — in-process trait (v0.1.0)

In v0.1.0 the AVM is an **in-process Rust trait** living inside `lesearch-core`. No separate process, no UDS, no MessagePack. Lives in the same address space as the daemon.

```rust
#[async_trait]
pub trait PolicyEngine: Send + Sync + 'static {
    async fn evaluate(&self, ctx: &ToolCallCtx) -> Decision;
}

pub enum Decision { Allow, Deny(Reason), AuditOnly(Reason) }
```

**Per-provider `enforcement_mode`** declared in the provider manifest:

- `strict` — daemon intercepts every pre-execution tool event and applies the engine's decision (Claude Code, OpenCode-MCP)
- `audit-only` — daemon cannot reliably block; engine runs anyway and the would-have-denied event is logged with a loud warning at agent spawn (Codex CLI)

**Phase B may split** the engine into a separate `lesearch-avm` sidecar process listening on UDS + MessagePack. Reasons to split *later*:

- Crash isolation (engine can restart without taking the daemon down)
- Run AVM as root for kernel-level policy hooks on Linux while daemon stays user-level
- Clean license boundary if AVM ever needs GPL-incompatible tooling

None of these reasons are blocking for v0.1.0. Splitting now is premature optimization (per A4).

### 2.3 L2 — Storage (StorageBackend trait)

```rust
#[async_trait]
pub trait StorageBackend: Send + Sync + 'static {
    async fn create_namespace(&self, agent_id: &AgentId) -> Result<NamespaceHandle>;
    async fn delete_namespace(&self, agent_id: &AgentId) -> Result<()>;
    async fn share_namespace(&self, ws: &WorkspaceId, agents: &[AgentId]) -> Result<()>;
}
```

**v0.1.0 default — `PlainDirBackend`**:
- Path: `$LESEARCH_HOME/agents/{agent_id}/fs/` with `chmod 700`
- Owner: daemon's effective user
- Cross-agent isolation: OS file permissions
- Pros: zero new dependencies; works everywhere; familiar to ops; survives daemon crash

**Opt-in (Phase B / `--features agentfs`) — `AgentFsBackend`**:
- Path: same logical layout, but namespace is a SQLite DB at `$LESEARCH_HOME/agents/{agent_id}/fs.sqlite`
- Mounted at runtime: FUSE on Linux, NFS on macOS, in-process SDK fallback if mounting refused
- Agent process `cwd` is the mount point → isolation boundary
- Enabled only when the upstream Turso AgentFS SDK reaches production stability

**Session log**:
- Path: `$LESEARCH_HOME/agents/{agent_id}/sessions/{session_id}.jsonl`
- Append-only, one CloudEvent per line, each Ed25519-signed
- Event types: `session.started`, `prompt.submitted`, `stream.chunk`, `tool.call.requested`, `tool.call.decided`, `tool.call.result`, `permission.requested`, `permission.decided`, `session.stopped`
- Rotation: 100 MB per file by default; `sessions-{n}.jsonl.zst` after rotation

**FTS5 mirror**:
- Path: `$LESEARCH_HOME/index.sqlite`
- Incrementally updated by `lesearch-storage` on every append
- Used for substring queries (`lesearch sessions grep`); jsongrep operates on raw JSONL

**Keyring** (OpenFused-compat on-disk format):
- Path: `$LESEARCH_HOME/.keys/`
- Files: `signing.key` (Ed25519), `encryption.key` (age/X25519), `peers.json`
- Permissions: `chmod 600`
- Format: OpenFused-compatible — a user can point `openfuse` at the same dir

### 2.4 L2′ — A2A Gateway

Inside the daemon, handled by the `lesearch-a2a` crate. Exposes:

| Endpoint | Method | Auth | Purpose |
|---|---|---|---|
| `/.well-known/agent-card.json` | GET | None | A2A agent discovery (returns card for default agent or specified via `?agent=` query) |
| `/profile` | GET | None | `PROFILE.md` (OpenFused compat) |
| `/config` | GET | None | Public keys |
| `/message/send` | POST | Bearer | Create A2A task against an agent |
| `/message/stream` | POST | Bearer | Create task + SSE stream |
| `/tasks` | GET | Bearer | List tasks |
| `/tasks/{id}` | GET | Bearer | Get task |
| `/tasks/{id}/cancel` | POST | Bearer | Cancel task |
| `/inbox` | POST | Ed25519 sig | OpenFused-compat signed inbox |
| `/outbox/{name}` | GET | Ed25519 challenge | OpenFused-compat outbox pull |

Bearer tokens stored in OS keychain; body size capped at 1 MB; SSE timeout 30 min.

### 2.5 L1 — Providers

Each provider is an async Rust trait impl:

```rust
#[async_trait]
pub trait AgentProvider: Send + Sync + 'static {
    fn manifest(&self) -> &ProviderManifest;
    async fn spawn(&self, spec: AgentSpec) -> Result<AgentHandle>;
}

pub struct AgentHandle {
    pub id: AgentId,
    pub stdin:  Option<DynAsyncWrite>,
    pub events: mpsc::Receiver<AgentEvent>,
    pub wait:   JoinHandle<ExitStatus>,
}
```

`AgentEvent` enum: `Stream(bytes)`, `ToolCallRequested(ToolCall)`, `ToolCallResult(...)`, `PermissionRequested(...)`, `SessionEnded(ExitStatus)`.

### 2.6 L5 — Transport

Three concrete transports implementing one trait:

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    async fn accept(&self) -> Result<Box<dyn ClientChannel>>;
}
```

- `direct` — binds `127.0.0.1:6767`; accepts any loopback connection (Docker-equivalent trust model)
- `ziti` — attaches to Ziti identity; accepts Ziti service binds
- `noise-ws` — accepts WebSocket upgrades; performs Noise handshake before delivering JSON-RPC frames

### 2.7 L6 — Clients

- **CLI** (`lesearch`): Rust, `clap`, Docker-style verbs. Connects to daemon over `direct` by default.
- **Tauri desktop** (Phase A): Rust core + React UI. Ships daemon as sidecar. macOS codesign + notary.
- **Web console**: React served from daemon at `/`. Works only with `direct` transport.
- **Native macOS app** (Phase B): SwiftUI, own LaunchAgent-wrapped daemon, menu bar. CGVirtualDisplay remains a research track only — never a v0.1.0 / v1.0 ship dependency.
- **Native iOS app** (Phase B): SwiftUI, Ziti iOS SDK, Secure Enclave identity, background refresh.

## 3.0 Life of a Command (added per Gemini gap + Synthesis §4)

End-to-end trace of `lesearch run --provider claude "summarize this directory"` on v0.1.0 (loopback only). Cite the exact crate at each hop.

1. **CLI** (`lesearch-cli`): parses `clap` args; resolves `$LESEARCH_HOME`; reads `~/.lesearch/config.toml`.
2. **Bearer-token mint**: `lesearch-cli` requests a per-session bearer from a UDS handshake on `$LESEARCH_HOME/cli.sock` (loopback ≠ trust per FR-40); the daemon emits a one-shot token bound to this PID + Origin = `lesearch-cli`.
3. **WebSocket dial**: `lesearch-cli` opens `ws://127.0.0.1:6767/ws`, sends `hello` JSON-RPC frame `{ client_id, version, capabilities, auth: { bearer } }`; daemon replies `welcome` with negotiated `A2A-Version` + capability set.
4. **`agent.create` JSON-RPC**: client → `lesearch-core` protocol router → agent-manager state machine. Agent transitions to `initializing`.
5. **Storage namespace**: agent-manager calls `StorageBackend::create_namespace(agent_id)` on the active backend (`PlainDirBackend` by default) → `mkdir -m 700 $LESEARCH_HOME/agents/{id}/fs`. Returns `NamespaceHandle { cwd }`.
6. **Policy preflight**: agent-manager calls `PolicyEngine::evaluate(ToolCallCtx::AgentSpawn { provider: "claude", spec })`. Engine consults baseline policy + per-provider `enforcement_mode` (Claude Code = `strict`; Codex = `audit-only`). Returns `Allow`.
7. **Provider spawn**: `lesearch-providers::claude::spawn(spec)` forks a `claude` CLI subprocess wired through `portable-pty` + `alacritty_terminal` headless buffer; `cwd = NamespaceHandle.cwd`.
8. **First session event**: `lesearch-storage::session_writer` writes `session.started` CloudEvent to `$LESEARCH_HOME/agents/{id}/sessions/{sid}.jsonl`. The event is JCS-canonicalized, gets a `prev_hash` field (zero-hash for the first event), and is Ed25519-signed. Signature covers the canonical bytes including `prev_hash`.
9. **FTS5 index update**: `lesearch-storage::fts_indexer` reads the appended event and updates the `index.sqlite` mirror.
10. **Stream loop**: PTY bytes → `lesearch-core` agent task → `tokio::sync::broadcast` fan-out → WebSocket binary frames (chan 1) → `lesearch-cli` stdout.
11. **Tool call**: when Claude emits a `tool.call.requested` MCP event, agent-manager calls `PolicyEngine::evaluate` again. Decision is appended to the session log as `tool.call.decided` (with `prev_hash` linking to the prior event). On `Allow`, daemon forwards to provider; on `Deny`, daemon returns the deny + reason to provider; on `AuditOnly`, daemon forwards but logs `would-have-denied` with a loud warning marker.
12. **Termination**: PTY EOF → agent transitions to `closed` → `session.stopped` event written → segment manifest emitted at log rotation OR at agent close (whichever comes first). Manifest covers `start_hash`, `end_hash`, `start_event_id`, `end_event_id` and is itself Ed25519-signed.

> Every hop above is in-process. There is no UDS, no MessagePack RPC, no separate AVM sidecar in v0.1.0. Phase B may externalize AVM + add Ziti transport without changing the CLI/protocol contract.

## 3. Wire Protocol (v0.1)

### 3.1 Framing

WebSocket text frames for JSON-RPC. Binary frames for mux:

```
+---------+----------+---------------+
| u8 chan | u8 flags | bytes payload |
+---------+----------+---------------+
```

- chan 0 = control (unused in binary; reserved)
- chan 1 = terminal data (agent → client stream, client → agent input)
- chan 2+ = reserved for multi-view future

### 3.2 JSON-RPC methods (subset; full list in `lesearch-wire-protocol.md`)

**Handshake**
- `hello` (client → server): `{ client_id, version, capabilities, auth: { bearer: "<per-session token>" } }` — `auth.bearer` is REQUIRED on every connection (FR-40); Origin and Host headers MUST be present and on the daemon's allowlist
- `welcome` (server → client): `{ server_id, version, session_id, capabilities, a2a_version: "1.0.0" }` — `a2a_version` advertises the negotiated A2A spec version per C-12 (A2A 1.0.0 default; 0.2.x via adapter shim with a deprecation warning)

**Agent lifecycle**
- `agent.create` → `{ agent_id }`
- `agent.list` → `[ { agent_id, status, provider, cwd, started, … } ]`
- `agent.attach` → subscribes to events for an agent
- `agent.send` → sends a new prompt to an agent
- `agent.stop` → sends SIGTERM then SIGKILL

**Session**
- `sessions.list`
- `sessions.search` (returns iterator cursor)
- `sessions.get`
- `sessions.verify`
- `sessions.replay` → creates new agent
- `sessions.export`

**Policy**
- `policy.evaluate` (internal; in-process trait call inside the daemon in v0.1.0; daemon → AVM sidecar over UDS only if Phase B splits the engine — never on WebSocket)
- `policy.reload` (client → daemon)
- `policy.status`

**Notifications (server → client)**
- `agent.event` — `{ agent_id, event }`
- `permission.request` — `{ agent_id, tool_call }`

### 3.3 Backward compatibility rules

- Any new field MUST be `Optional` with a default
- Never remove a field — deprecate by stopping to emit it
- Never narrow a type (string → enum; nullable → non-null)
- Versions are monotone; server advertises capability set in `welcome`

## 4. Data Model

### 4.1 Session event (CloudEvent envelope) — JCS-canonical + hash-chained + Ed25519-signed (per A8)

```json
{
  "specversion": "1.0",
  "id": "01HXZ...",
  "source": "lesearch://daemon/{daemon_id}",
  "type": "lesearch.tool.call.requested.v1",
  "subject": "agent:{agent_id}",
  "time": "2026-04-14T20:15:00Z",
  "datacontenttype": "application/json",
  "data": { "tool": "Bash", "args": { "command": "ls -la" } },
  "prev_hash": "sha256-base64:abc...",
  "signature": "ed25519:{base64}",
  "key_fingerprint": "ed25519-pub-fingerprint:...",
  "signed_over": ["specversion","id","source","type","subject","time","data","prev_hash"]
}
```

**Canonicalization (RFC 8785 / JCS)**: every event is serialized using JSON Canonicalization Scheme **before** hashing and signing. This eliminates whitespace + key-order ambiguity attacks where an attacker re-serializes the JSON to break the verifier without breaking the signature.

**Hash chain**: every event carries `prev_hash = SHA-256(canonical_bytes_of_prior_event)` (or `0…0` for the first event in a file). Deletion or reordering of any record breaks the chain on the next `verify`.

**Append-only at FS level**: `O_APPEND` open flag prevents in-place truncation; rotation produces a fresh file + a signed segment manifest (see §4.4 below).

**Tamper evidence is now four-layered**:
1. JCS canonicalization → no re-serialization attacks
2. Per-event Ed25519 signature → no field mutation
3. Hash chain → no record deletion/reordering inside a file
4. Signed segment manifests + key-rotation transition manifests → no whole-file replacement; key-compromise window is bounded

Append-only is still enforced at the file-system level through the `O_APPEND` open flag.

### 4.2 Agent registry (SQLite at `$LESEARCH_HOME/registry.sqlite`)

```
Table agents
  id             TEXT PRIMARY KEY
  provider       TEXT
  status         TEXT CHECK (status IN ('initializing','idle','running','stopped','error'))
  cwd            TEXT
  created_at     INTEGER
  last_active_at INTEGER
  config_json    TEXT
  owner_pubkey   TEXT
  policy_name    TEXT

Table sessions
  id          TEXT PRIMARY KEY
  agent_id    TEXT REFERENCES agents(id)
  started_at  INTEGER
  stopped_at  INTEGER
  status      TEXT
  file_path   TEXT
```

### 4.3 Config schema (`~/.lesearch/config.toml`)

```toml
[daemon]
home = "~/.lesearch"
log_level = "info"

[transport]
mode = "direct"           # "direct" | "ziti" | "noise-ws"

[limits]
max_concurrent_agents = 10
max_daemon_memory_mb  = 500
max_agent_memory_mb   = 2048
session_retention_days = 90
storage_warn_gb = 20

[providers]
claude   = { path = "claude", enabled = true }
codex    = { path = "codex",  enabled = true }
opencode = { path = "opencode", enabled = false }
gemini   = { path = "gemini", enabled = false }

[a2a]
enabled = false
bearer_token_env = "LESEARCH_A2A_TOKEN"
spec_version = "1.0.0"    # negotiated via A2A-Version header

[auth]                    # localhost hardening per FR-40
bearer_token_env = "LESEARCH_BEARER_TOKEN"     # default: per-session token minted at handshake
origin_allowlist = ["http://127.0.0.1:6767", "tauri://localhost"]
sensitive_ops_require_os_auth = true            # Touch ID on macOS / sudo on Linux for policy edit, key rotation, uninstall

[otel]
endpoint = ""             # OFF by default — set to enable OpenTelemetry export
```

### 4.4 Segment manifest schema (added per A8)

A signed segment manifest is emitted whenever a session log file is rotated, when an agent closes, and on key rotation:

```json
{
  "segment_id": "01HXZ-segment-1",
  "agent_id": "01HXZ-agent",
  "file_path": "$LESEARCH_HOME/agents/{id}/sessions/{sid}.jsonl",
  "start_event_id": "01HXZ-event-first",
  "end_event_id":   "01HXZ-event-last",
  "start_hash": "sha256-base64:000…",
  "end_hash":   "sha256-base64:zzz…",
  "event_count": 4321,
  "key_fingerprint": "ed25519-pub-fingerprint:...",
  "signed_at": "2026-04-14T22:15:00Z",
  "signature": "ed25519:{base64 over JCS(canonical_bytes minus signature)}"
}
```

**Key rotation transition manifest** binds an old key fingerprint to a new one with both signatures so that `verify` can prove continuity:

```json
{
  "type": "key.rotation.transition.v1",
  "old_fingerprint": "...",
  "new_fingerprint": "...",
  "rotated_at": "2026-04-14T22:30:00Z",
  "signed_by_old": "ed25519:{base64}",
  "signed_by_new": "ed25519:{base64}"
}
```

`lesearch sessions verify` walks the per-event hash chain, validates per-event signatures, and then walks the segment manifest chain (including transition manifests) end-to-end. Any break in any layer is reported with file + line + which layer broke.

## 5. Sequence Diagrams (text form)

### 5.1 Spawn an agent

```
User           CLI               Daemon              AVM            Provider      StorageBackend
 |  lesearch run claude "..."     |                   |               |             |
 |------------>|                  |                   |               |             |
 |             |  hello           |                   |               |             |
 |             |----------------->|                   |               |             |
 |             |  welcome         |                   |               |             |
 |             |<-----------------|                   |               |             |
 |             |  agent.create    |                   |               |             |
 |             |----------------->|                   |               |             |
 |             |                  |  create namespace |               |             |
 |             |                  |-------------------------------------------------->|
 |             |                  |                   |               |   ok        |
 |             |                  |<--------------------------------------------------|
 |             |                  |  policy.evaluate  |               |             |
 |             |                  |------------------>|               |             |
 |             |                  |       allow       |               |             |
 |             |                  |<------------------|               |             |
 |             |                  |  spawn provider   |               |             |
 |             |                  |---------------------------------->|             |
 |             |                  |  session.started  |               |             |
 |             |                  |  (write to JSONL) |               |             |
 |             |  { agent_id }    |                   |               |             |
 |             |<-----------------|                   |               |             |
 |  output ... |                  |                   |               |             |
```

### 5.2 Remote pair (Phase B)

```
User           iOS App            Daemon             Ziti Controller    Keyring
 |  lesearch daemon pair          |                        |               |
 |------------------------------->|                        |               |
 |                   QR code      |                        |               |
 |<-------------------------------|                        |               |
 |  scan QR         |             |                        |               |
 |----------------->|             |                        |               |
 |                  |  enroll(JWT)|                        |               |
 |                  |-------------------------------------->|               |
 |                  |       identity cert                   |               |
 |                  |<--------------------------------------|               |
 |                  |  add pubkey(phone)                    |               |
 |                  |--------------------------------------------------->   |
 |                  |  open Ziti dial                       |               |
 |                  |<--------------------------------------|               |
 |                  |  hello+E2E    |                       |               |
 |                  |--------------->                       |               |
 |                  |  welcome      |                       |               |
 |                  |<---------------                       |               |
```

### 5.3 Tool call with policy

```
Provider         Daemon              AVM             Session JSONL
   | tool.call.requested              |                     |
   |--------------->|                 |                     |
   |                |  policy.evaluate|                     |
   |                |---------------->|                     |
   |                |   decision      |                     |
   |                |<----------------|                     |
   |                |  append "tool.call.decided"           |
   |                |-------------------------------------->|
   |  allow/deny    |                 |                     |
   |<---------------|                 |                     |
```

## 6. Concurrency Model

- One tokio runtime in the daemon (`#[tokio::main(flavor = "multi_thread")]`).
- Per-agent actor: each agent lives in its own `tokio::spawn` task holding a `mpsc::Receiver` of commands. Messages are the only way in.
- Fan-out for subscribers: `tokio::sync::broadcast` per agent for `AgentEvent` subscribers (clients).
- Blocking work: session log writes use `spawn_blocking`; AgentFS I/O through its async SDK.
- Backpressure: event channels have bounded capacity; if a slow client can't keep up, oldest events are dropped with a `stream.gap` marker recorded in the session log.

## 7. Error Handling

- All errors typed via `thiserror` in crates, flattened to `anyhow::Error` at crate boundary for CLI only.
- Every user-visible error carries a stable error code (`ERR_1001`…) and a doc URL pointer.
- The daemon never exits on recoverable errors; agent-level failures stay scoped to that agent.
- Unrecoverable errors (FS corruption, crypto failure) fail the daemon fast with a clear message + where state is recovered from.

## 8. Security Design

### 8.1 Trust boundaries

> **Loopback ≠ trust** (per A9). Browser tabs, malicious extensions, supply-chain compromise, and same-user hostile processes all reach `127.0.0.1` for free. Treat every loopback request as untrusted.

| Boundary | Control |
|---|---|
| User ↔ Daemon (local) | Loopback + **per-session bearer token on every WS + HTTP request (FR-40)** + **Origin/Host allowlist (default deny)** + **OS-level auth (Touch ID / `sudo`) for sensitive ops** (policy edit, key rotation, transport mode change, uninstall) |
| Daemon ↔ in-process AVM | Same address space; trait call (no IPC). Phase B sidecar split would gain a UDS `chmod 600` socket. |
| Daemon ↔ Provider | stdio to user-owned subprocess; provider's own auth handles its API |
| Daemon ↔ Client (remote, Phase B) | Ziti mTLS OR Noise-WS E2E + bearer; A2A bearer token |
| Client ↔ Client | Never direct; always through daemon |

### 8.2 Key management

- Daemon key: generated on first run, Ed25519 + X25519; stored `chmod 600` in `$LESEARCH_HOME/.keys/`
- Optional integration with macOS Keychain / Linux Secret Service / Windows DPAPI (Phase B)
- Rotation: `lesearch keys rotate` writes a transition-signed manifest; old key stays valid for 24 h for in-flight sessions
- Revocation: `lesearch keys revoke <fingerprint>` writes a revocation record to keyring + pushes to paired peers

### 8.3 Sandboxing

- Agent `cwd` is the StorageBackend namespace root (`PlainDirBackend` `chmod 700` dir by default; AgentFS mount when opt-in) → file access isolation
- In-process AVM trait blocks commands by pattern before execution where the provider's tool events are interceptable (`enforcement_mode: strict`); audit-only otherwise
- **Network egress policy is DEFERRED**: PID-scoped `pf` is unreliable on macOS (per Realities.3). Linux cgroup + iptables remains a Phase B research track. Do not promise per-process egress control in v0.1.0 marketing or docs.

## 9. Observability

- OpenTelemetry traces with `opentelemetry-rust` exporter
- Metrics: agent counts, tool-call rate, policy-decision rate, memory RSS, open FDs, mount count
- Structured logs in JSON at `info` level by default
- `lesearch doctor` aggregates health in one command
- agents-observe hook events emitted on agent/tool events for dashboard integration

## 10. Resource Hygiene (PRD-NFR-1..8)

Concrete implementation plan for the user-flagged critical requirements:

| NFR | Implementation |
|---|---|
| Idle RAM ≤ 50 MB | Lazy-init of heavy modules; tokio runtime with `worker_threads = 2` when idle; no in-memory caches for FTS (SQLite handles it) |
| Idle CPU ≤ 0.1% | Background tasks use tokio timers with ≥ 30s intervals when no clients connected; suspend timers entirely after 5 min idle |
| Per-agent RAM overhead ≤ 20 MB | Bounded event-buffer channels; drop-oldest backpressure; xterm-headless buffer capped at N rows |
| Disk ≤ 500 MB (excl user data) | Rust binary + web console < 80 MB typical; index fragmentation handled by periodic `VACUUM` |
| Session log rotation | `O_APPEND` + file size check on every N events |
| `lesearch doctor` < 2 s | All metrics precomputed; `doctor` reads them from shared state |
| Zero zombies on shutdown | Tokio graceful shutdown guard kills all `tokio::process::Child` handles |
| Clean uninstall | `lesearch uninstall` walks LaunchAgent/systemd unit paths, AgentFS mount points, removes all |

## 11. Trade-offs & Decisions

### Why Rust for the daemon (vs Node/paseo)
+ Single static binary for end users → no runtime install friction
+ Memory safety for a long-running background service
+ First-class Ziti Rust SDK
+ karna + cmux AVM already Rust — no cross-language IPC headaches later
– Smaller pool of contributors than Node
– Slower to iterate early vs TypeScript

### Why Tauri for Phase A (vs Electron)
+ Uses system WebView (tiny binary, ~10 MB vs Electron's 100 MB+)
+ Rust core → shares state with daemon naturally
+ macOS + Linux + Windows from one build
– Slightly less mature for complex UI, but acceptable for dashboards

### Why a `StorageBackend` trait (vs hardcoding any single backend)
+ Plain directories ship in v0.1.0 — zero new deps, works everywhere, familiar to ops
+ AgentFS (SQLite-backed → portable, durable, snapshot-friendly) is opt-in via `--features agentfs` when its SDK matures
+ Future backends (overlayfs, ZFS datasets) plug in without touching the daemon
+ Trivial sandbox boundary remains: a different namespace per agent, regardless of backend
– A trait adds one indirection vs hardcoded calls — negligible cost at the rate we touch it

### Why node-pty-equivalent + alacritty_terminal vs tmux
+ No subprocess dep for end users
+ Cross-platform (including Windows via ConPTY)
+ Agent events already modeled in-memory; tmux would be a second source of truth
– Lose "daemon restart, session survives" unless we add tmux mode as opt-in (planned in DP2)

### Why JSONL + jsongrep + FTS5 (vs Postgres or Elasticsearch)
+ Zero install — SQLite is everywhere
+ JSONL is append-only and stream-friendly — no index corruption on crash
+ jsongrep gives regex-over-paths which jq / Elastic don't natively
– Limits us to single-host search; multi-daemon federated search is Phase B

## 12. Performance Budget

All budgets are enforced by the **CI benchmark harness (NFR-39)**. PRs fail if any p99 exceeds its budget. Daemon-alone metrics are tracked separately from Tauri-shell metrics (per A11).

| Hot path | Scope | Budget (p99) | Why |
|---|---|---|---|
| PTY byte → WebSocket byte (loopback) | Daemon-alone | ≤ 1 ms | User feels it |
| PTY byte → WebSocket byte (Ziti LTE, Phase B) | Daemon + transport | ≤ 150 ms | LTE RTT floor |
| Tool-call policy decision (in-process trait, v0.1.0) | Daemon-alone | ≤ 200 µs | Direct trait call, no IPC |
| Tool-call policy decision (UDS sidecar, Phase B) | Daemon + sidecar | ≤ 5 ms | UDS round-trip |
| Session event write (JCS canonicalize + sign + fsync, batch) | Daemon-alone | ≤ 2 ms average | Amortized by batch |
| Session FTS query (1 GB history) | Daemon-alone | ≤ 100 ms | SQLite FTS5 |
| Session jsongrep query (1 GB history, Phase B) | Daemon-alone | ≤ 500 ms | DFA linear scan |
| `agent.create` end-to-end | Daemon-alone | ≤ 2 s | PTY spawn + MCP handshake + storage init |
| Tauri shell cold start | Tauri shell | ≤ 3 s | 2023+ Apple Silicon; reported separately |
| **CI benchmark harness fail-open threshold** | All metrics | Budget +10% for 1 PR; fail on the next | Avoids flapping on transient noise |

## 13. Capacity Planning

- 10 concurrent agents per daemon default ceiling
- 1 session log per agent, rotated at 100 MB → typical session log ≤ 5 MB
- FTS5 index grows at ~25% of raw JSONL size → plan for ~500 MB index per 2 GB history
- AgentFS per agent typically < 100 MB for code+build artifacts; user should monitor

## 14. Failure Modes & Recovery

| Failure | Detection | Recovery |
|---|---|---|
| Daemon OOM | OS kill; config `max_daemon_memory_mb` preemptively refuses new agents | systemd/launchd restarts; registry replay restores agent states; sessions marked `recovered` |
| Agent provider crash | stdio EOF | Agent transitions `running → error`; event emitted; user notified |
| Agent hang | timeout on `PermissionRequested` > 1 h (default) | Auto-deny with audit event; admin override possible |
| AVM panic (in-process trait, v0.1.0) | Tokio task panic | Daemon catches via `catch_unwind` boundary; falls back to `deny-all` for sensitive tools; emits loud audit event; restarts engine task |
| AVM sidecar crash (Phase B sidecar mode only) | UDS EOF | Daemon falls back to `deny-all` for sensitive tools; logs loudly |
| Disk full | Write fails | New sessions refused; `doctor` flags; existing agents marked `paused` with retry policy |
| Clock skew breaking audit sigs | Sig verification fails | `lesearch doctor` flags; events still stored but flagged |
| Network loss (remote client) | WebSocket close | Client reconnects exponential backoff up to 30 s; local daemon + agent unaffected |
| Bearer token leaked (FR-40) | User reports OR audit shows unexpected Origin / unrecognized session | `lesearch keys rotate` invalidates all bearer tokens; all clients must re-handshake; audit event emitted; segment manifest captures the rotation boundary |
| Origin spoofing attempt | Daemon rejects request as Origin not on allowlist | Audit event emitted with rejected Origin + Host; rate-limit threshold trips after N rejections from same client |

## 15. Open Design Questions (for team review)

Q-D1 — Should the daemon expose its HTTP surface (A2A + web console) on a different port from WebSocket? Separating isolates DoS surfaces but complicates config.
~~Q-D2 — Is the `Transport` trait too coarse? Consider splitting `listen` from `accept_client` to allow multi-transport daemon (Ziti AND loopback both active).~~ **SUPERSEDED (A5)**: v0.1.0 ships only the loopback transport — single-transport simplification stands until Phase B reintroduces remote.
Q-D3 — Should session events use JSON Web Signatures (JWS) instead of raw Ed25519 over CloudEvents? JWS is more standardized but adds deps. **(Note: JCS canonicalization per A8 already addresses the strongest reason to adopt JWS.)**
Q-D4 — Should the FTS5 mirror be rebuildable from JSONL (true) or authoritative (false)? Rebuildable is simpler but slower on boot.
~~Q-D5 — Do we bake `openfused` as a hard Rust crate dependency, or reimplement the on-disk format ourselves for control?~~ **RESOLVED (A2 + Realities.10)**: optional integration only (compatible on-disk format), never a hard dependency. Keeps `lesearch-storage` independent.
Q-D6 — Should we support a MCP proxy mode where the daemon is itself an MCP server other agents connect to?

## 16. Revision History

| Rev | Date | Notes |
|---|---|---|
| 0.1 | 2026-04-14 | Initial system design for multi-agent review. |
| 0.2 | 2026-04-14 | Review synthesis applied (A1–A12, D1–D5). §1 diagram redrawn (in-process AVM, StorageBackend trait, loopback-only L5). §2.1 collapsed to 5–7 crates. §2.2 AVM rewritten as in-process trait. §2.3 storage rewritten around StorageBackend trait. New §3.0 Life of a Command. §3.2 hello includes bearer + A2A-Version. §4.1 adds JCS + prev_hash + key_fingerprint. New §4.4 segment manifest schema. §4.3 config gains [auth] section. §8.1 trust boundaries hardened (loopback ≠ trust). §8.3 PID-pf egress dropped. §11 trade-off reframed as StorageBackend trait. §12 perf budget split daemon vs Tauri + CI bench. §14 adds bearer-leak + Origin-spoof rows. §15 Q-D2/Q-D5 resolved. |

