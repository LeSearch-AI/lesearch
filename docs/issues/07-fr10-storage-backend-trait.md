# Issue: FR-10 extract `StorageBackend` trait — pluggable storage layer

**Labels:** `area/storage` `type/refactor` `priority/high` `release-gate/v0.1.0` `phase/A.3`
**Milestone:** v0.1.0 Daily Driver
**FR refs:** FR-10 (PRD rev-0.2)
**Story refs:** S-7.1, S-7.2
**Blocked by:** PR #3 (v0.1.0 scaffold)
**Blocks:** Release-gate certification; any future `AgentFsBackend` work

## Summary

Rev-0.2 A1 locked in storage as a pluggable trait (`StorageBackend`) with `PlainDirBackend` as default and `AgentFsBackend` as a future opt-in (`--features agentfs`). The current scaffold wires `lesearch-storage` modules directly — registry, keyring, session_log, search are all concrete types. Extract the trait so Phase B can swap in AgentFS without touching the daemon.

## Acceptance criteria

- [ ] New trait `StorageBackend` in `lesearch-storage/src/backend.rs` with methods:
  - `fn open(home: &Path) -> Result<Self>`
  - `fn agent_dir(&self, agent_id: &AgentId) -> Result<PathBuf>`
  - `fn session_writer(&self, session_id: &SessionId) -> Result<Box<dyn SessionWriter>>`
  - `fn keyring(&self) -> &Keyring`
  - `fn registry(&self) -> &Registry`
  - `fn search(&self) -> &SearchIndex`
- [ ] `PlainDirBackend` implementation wrapping the current modules (default)
- [ ] `AgentFsBackend` stub gated behind `--features agentfs` that returns `not_implemented` errors for now
- [ ] `DaemonState` holds `Box<dyn StorageBackend>` instead of concrete types
- [ ] `Cargo.toml` feature flags: `[features] default = []; agentfs = ["dep:agentfs-sdk"]`
- [ ] `lesearch doctor` reports active backend name
- [ ] Existing 19 storage tests continue to pass
- [ ] New trait-level tests (at least 3) exercising `PlainDirBackend` via the trait

## Non-goals

- Implementing AgentFS functionality (Phase B)
- Runtime backend switching (compile-time feature gate only)
- Migration tooling between backends (Phase B)

## Implementation notes

- This is a pure refactor — no behavior change
- The `StorageBackend` methods should all be `&self` (no `&mut`) — internal concurrency uses `Mutex`/`RwLock`
- `SessionWriter` stays trait-based so the daemon's background task can be generic over it
- Optional: mark `PlainDirBackend` with `#[non_exhaustive]` to reserve room for additional fields

## References

- `docs/PRD.md` rev-0.2 §FR-10
- `docs/STORAGE_MODEL.md` §AgentFS Integration (L0)
- `docs/SYSTEM_DESIGN.md` §2.3
- `docs/REVIEW_SYNTHESIS.md` §A1
