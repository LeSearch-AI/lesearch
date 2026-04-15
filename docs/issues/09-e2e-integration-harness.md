# Issue: End-to-end integration test harness

**Labels:** `area/testing` `type/feature` `priority/high` `release-gate/v0.1.0` `phase/A.4`
**Milestone:** v0.1.0 Daily Driver
**NFR refs:** NFR-39 (CI benchmark harness), NFR-11 (cold-start + handshake)
**Blocked by:** PR #3 (v0.1.0 scaffold)
**Blocks:** Release-gate criteria #1, #2, #5 (provider coverage, 7-day soak, crash recovery)

## Summary

All 60 current tests are per-crate unit tests. Build a workspace-level integration harness that binds the daemon on an ephemeral port, drives it through the WebSocket protocol with a mock provider, and asserts full-stack behavior: handshake → spawn → stream events → stop → session JSONL signed + hash-chained → search hit → verify pass.

## Acceptance criteria

- [ ] New crate `crates/lesearch-testutil` (or `tests/` directory at workspace root) with:
  - `EphemeralDaemon::spawn()` helper — binds daemon to `127.0.0.1:0`, returns port + `TempDir` `$LESEARCH_HOME`
  - `WsClient` thin wrapper for test-side JSON-RPC
  - `MockProvider` that emits scripted `AgentEvent` sequences (no real CLI subprocess)
- [ ] Integration tests in `tests/e2e/`:
  - `happy_path.rs` — handshake + spawn + input + events + stop + session verify
  - `persistence.rs` — spawn agent, kill daemon, restart daemon, session log loads + verifies
  - `concurrent_agents.rs` — 5 parallel agents, all log to separate dirs, no cross-talk
  - `search_roundtrip.rs` — spawn + inject 100 events + `sessions.search` returns expected hit count
  - `crash_recovery.rs` — simulate mid-session crash (SIGKILL), restart, assert JSONL hash chain still verifies
  - `bearer_required.rs` — (after issue #06 lands) reject connection without bearer
- [ ] CI workflow `.github/workflows/e2e.yml` runs harness on macOS + Linux
- [ ] All e2e tests < 10s wall time total (parallel execution)
- [ ] `cargo test --workspace --all-features` includes e2e by default

## Non-goals

- Testing against real `claude` / `codex` binaries (separate smoke-test PR per issue #11)
- Cross-version daemon compat tests (covered by issue #08)
- Fuzzing (Phase B)
- Performance benchmarks (NFR-39 CI benchmark suite is a separate issue)

## Implementation notes

- Ephemeral port: bind to `127.0.0.1:0`, read actual port from listener
- `TempDir` from `tempfile` crate cleans up `$LESEARCH_HOME` automatically
- `MockProvider` implements `AgentProvider` trait with scripted behavior — no PTY, just emits events on a schedule
- Use `tokio::test` with `#[tokio::test(flavor = "multi_thread")]` for parallel-agent tests
- Integration test for bearer auth — wait for issue #06 to merge, then add

## References

- `docs/PRD.md` rev-0.2 §8.1 (Release criteria)
- `docs/SYSTEM_DESIGN.md` §12 (testing strategy)
- [Rust book: Integration tests](https://doc.rust-lang.org/book/ch11-03-test-organization.html#integration-tests)
- [`tempfile` crate](https://crates.io/crates/tempfile)
