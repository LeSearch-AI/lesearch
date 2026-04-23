# Issue: FR-39 session integrity — signed segment manifests on rotation

**Labels:** `area/storage` `area/security` `type/feature` `priority/high` `release-gate/v0.1.0` `phase/A.3`
**Milestone:** v0.1.0 Daily Driver
**FR refs:** FR-39 (PRD rev-0.2)
**Story refs:** S-7.1, S-11.1
**Blocked by:** PR #3 (v0.1.0 scaffold)
**Blocks:** Release-gate certification (crash + power-loss recovery)

## Summary

Rev-0.2 A8 hardened the session log with JCS canonicalization + Ed25519 per-line signatures + `prev_hash` chain. The per-line chain is implemented. Missing: a **signed segment manifest** emitted on log rotation (e.g., every 100 MB or every N records) that captures the segment's first/last hash, record count, and a separate signature — so verification can proceed in chunks and rotation boundaries are themselves auditable.

## Acceptance criteria

- [ ] On rotation, `session_log.rs` emits a `manifest.json` file alongside the rotated JSONL segment:
  ```json
  {
    "segment_id": "<uuid-v7>",
    "session_id": "<uuid-v7>",
    "segment_file": "000001.jsonl",
    "first_record_hash": "<hex>",
    "last_record_hash": "<hex>",
    "record_count": 1247,
    "started_at": "2026-04-14T22:10:00Z",
    "ended_at": "2026-04-14T22:45:18Z",
    "signature": "<base64-ed25519>",
    "pubkey_fingerprint": "<hex>"
  }
  ```
- [ ] Manifest signature covers JCS-canonicalized form of all fields except `signature` itself
- [ ] `lesearch sessions verify <session_id>` extended to:
  - Load all segments (via manifests) in order
  - Verify each manifest signature
  - Verify `last_record_hash` of segment N == `prev_hash` of segment N+1's first record
  - Verify each JSONL record per existing logic
- [ ] On startup crash recovery: if a segment exists without a manifest (crash mid-write), daemon emits `manifest.json` for the partial segment, chaining to last verified record; logs a warning event to the session indicating recovery
- [ ] Key rotation transition manifest: when `Keyring` key rotates, emit a `transition.json` that binds old-key-fingerprint → new-key-fingerprint with both signatures
- [ ] 5+ new tests:
  - Fresh session writes initial manifest on first rotation
  - Cross-segment hash continuity verified
  - Corrupt segment detected (manifest sig verify fails)
  - Tampered record detected (chain breaks)
  - Key rotation produces valid transition manifest
  - Mid-write crash → partial segment recovery writes manifest with warning record

## Non-goals

- Encryption-at-rest (v0.2 optional)
- Merkle-tree segment summaries (v0.2 performance optimization)
- Cross-session manifest aggregation (Phase B)

## Implementation notes

- Rotation trigger: `max_log_bytes` per-agent config, default 100 MB
- Manifest emission is atomic — write `.tmp` first, `fsync`, rename
- Transition manifest lives at `$LESEARCH_HOME/keyring/transitions/<timestamp>.json`
- CLI `lesearch sessions verify --deep` runs full hash+manifest verification; `--fast` only checks manifests

## References

- `docs/PRD.md` rev-0.2 §FR-39
- `docs/STORAGE_MODEL.md` §Session Log Format + §Verification
- `docs/SYSTEM_DESIGN.md` §4.1 + §4.4 (segment manifest schema)
- `docs/REVIEW_SYNTHESIS.md` §A8
- [JCS RFC 8785](https://datatracker.ietf.org/doc/html/rfc8785)
