# Issue: FR-40 localhost hardening — per-session bearer + Origin/Host allowlist

**Labels:** `area/daemon` `area/security` `type/feature` `priority/critical` `release-gate/v0.1.0` `phase/A.2`
**Milestone:** v0.1.0 Daily Driver
**FR refs:** FR-40 (PRD rev-0.2)
**Story refs:** S-8.1, S-12.5
**Blocked by:** PR #3 (v0.1.0 scaffold)
**Blocks:** Release-gate certification

## Summary

The v0.1.0 release gate requires loopback-only hardening: loopback-bound ≠ loopback-trusted. Implement per-session bearer token authentication on the WebSocket endpoint, and an explicit Origin/Host allowlist to prevent DNS-rebinding and browser-extension attacks targeting the local daemon.

## Acceptance criteria

- [ ] Daemon generates a 64-byte random bearer token on startup if `$LESEARCH_HOME/keyring/session.token` doesn't exist
- [ ] Token file created with `0600` permissions; daemon aborts start if perms are looser
- [ ] WebSocket upgrade rejected with close code `1008` + `X-LeSearch-Error: -32000` if `Authorization: Bearer <token>` missing or wrong
- [ ] HTTP endpoints (`/health`, `/.well-known/agent.json`) unauthenticated (read-only, no state change)
- [ ] Origin/Host allowlist: daemon rejects connections where `Origin` or `Host` header isn't in `["127.0.0.1:6767", "localhost:6767"]` + any `config.toml` `[security] additional_origins` entries
- [ ] `lesearch` CLI automatically reads `$LESEARCH_HOME/keyring/session.token` and sends bearer on every WebSocket connection
- [ ] `lesearch daemon rotate-token` command regenerates the token (invalidates existing connections)
- [ ] New tests:
  - Valid bearer accepted → handshake succeeds
  - Missing bearer rejected → close 1008
  - Wrong bearer rejected → close 1008
  - Origin `http://evil.com` rejected → close 1008
  - Host `evil-attacker.local:6767` rejected → close 1008
  - Valid bearer + valid Origin → succeeds
- [ ] `tracing` filter redacts `Authorization` header values at span level (no bearer in logs)

## Non-goals

- mTLS (Phase B)
- OAuth2 / OIDC (Phase B)
- Multi-tenant token scoping (Phase B)
- Rate limiting per token (Phase B — only A2A routes get rate limits for now)

## Implementation notes

- New module: `crates/lesearch-daemon/src/auth.rs` with `BearerAuth` extractor
- Token generation: `rand::thread_rng()` → 64 bytes → hex-encoded to file
- Axum layer: apply to `/ws` only, leave `/health` and `/.well-known/agent.json` open
- Origin check happens before bearer check (cheap filter)
- CLI: `lesearch-cli/src/ws_client.rs` reads token from `$LESEARCH_HOME/keyring/session.token` (respect `LESEARCH_HOME` env var); fall back to `~/.lesearch/keyring/session.token`
- Redaction: `tracing_subscriber` filter spec `authorization=none` or custom `Fmt` layer that strips the header

## Security threat model addressed

| Attack | Mitigation |
|---|---|
| Local malware connects to loopback daemon | Must steal bearer token from `$LESEARCH_HOME/keyring/session.token` (requires file-read access) |
| Browser extension reaches `http://127.0.0.1:6767` via fetch | Origin header check blocks — extensions can't forge Origin |
| DNS rebinding (evil.com → 127.0.0.1) | Host header allowlist blocks |
| Logs leak bearer token to disk/stdout | Redaction filter strips `Authorization` |

## References

- `docs/PRD.md` rev-0.2 §FR-40
- `docs/A2A_FACADE.md` §Authentication (bearer token pattern)
- [OWASP: localhost-trust anti-pattern](https://owasp.org/www-community/attacks/Server_Side_Request_Forgery)
- [DNS rebinding attacks on local services](https://blog.mozilla.org/en/products/firefox/firefox-localhost-blocks/)
