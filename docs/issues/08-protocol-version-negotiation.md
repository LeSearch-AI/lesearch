# Issue: Protocol version negotiation in handshake — release gate #4

**Labels:** `area/protocol` `area/daemon` `type/feature` `priority/critical` `release-gate/v0.1.0` `phase/A.1`
**Milestone:** v0.1.0 Daily Driver
**FR refs:** §server.handshake (protocol-v0.1.md), §Versioning policy
**Blocked by:** PR #3 (v0.1.0 scaffold)
**Blocks:** Release-gate criterion #4 (cross-version protocol round-trip)

## Summary

Release gate criterion #4 requires that vN client and vN−1 daemon still handshake and drive an agent. The current `server.handshake` handler echoes a fixed `PROTOCOL_VERSION` constant with no negotiation. Add explicit version fields to the handshake payload, MAJOR-mismatch rejection, MINOR-skew warnings, and a CI test that pins older-version clients against newer daemons.

## Acceptance criteria

- [ ] `server.handshake` request includes:
  - `client_protocol_version: "0.1.0"` (SemVer string)
  - `client_name: string`
  - `client_version: string`
- [ ] `server.handshake` response includes:
  - `daemon_protocol_version: "0.1.0"`
  - `daemon_version: string`
  - `compatibility: { status: "compatible" | "minor_skew" | "incompatible", warnings: [string] }`
- [ ] MAJOR mismatch → handshake returns error `-32013 ProtocolIncompatible` and closes WS
- [ ] MINOR skew (client-minor < daemon-minor) → warn in `compatibility.warnings` but allow
- [ ] MINOR skew (client-minor > daemon-minor) → warn "daemon older than client; some methods may be unavailable" but allow
- [ ] Unknown method names return `-32601 MethodNotFound` with an additional `available_methods` array in error data (helps client skip features)
- [ ] Two new CI tests:
  - `vN client ↔ vN daemon` (baseline)
  - `vN-1 client ↔ vN daemon` (version skew — exercised by checking in a pinned "v0.0.9" client fixture)
- [ ] `CHANGELOG.md` entry template: every protocol change must list affected methods + compatibility impact

## Non-goals

- Full capability negotiation (Phase B — extension registry)
- `/ws/v2` path versioning (only needed at next MAJOR bump)
- Forward-compatibility past v1.0 (re-evaluate at that milestone)

## Implementation notes

- Add `semver` crate to workspace deps
- Parse versions at handshake, compare MAJOR, emit warnings for MINOR diff
- Test fixture: check in `tests/fixtures/client-v0.0.9-handshake.json` with expected response
- Integration test spawns daemon, sends the fixture handshake as raw JSON-RPC, asserts response shape
- `available_methods` array populated from the router's method-constant list (introspection)

## References

- `docs/protocol-v0.1.md` §2.1 (Versioning policy) + §3 (Handshake)
- `docs/PRD.md` rev-0.2 §8.1 (Release criteria #4)
- `docs/REVIEW_SYNTHESIS.md` §A10 (A2A 1.0.0 with version negotiation)
- [SemVer spec](https://semver.org/)
