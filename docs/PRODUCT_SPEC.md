# LeSearch — Product Spec

**Version**: 0.2 (review synthesis applied — A1–A12, D1–D5)
**Date**: 2026-04-14
**Owner**: Arya Teja Rudraraju
**Status**: Pre-implementation, ruthless v0.1.0 scope locked

---

## 1. Product One-Liner

**LeSearch — the open control plane for your CLI agents.** Self-hosted. Vendor-neutral. Works with Claude Code, Codex, OpenCode, and anything with a CLI. Search every session you've ever run. Eventually: control from your phone over a zero-trust overlay.

> "The open control plane for your CLI agents — self-hosted, vendor-neutral, session-searchable today; zero-trust mobile control on the roadmap."

## 2. The Problem

Developers running AI coding agents today face four trapped-in-silo problems:

1. **Agent silos** — Claude Code, Codex, OpenCode, Gemini CLI each live in separate UIs, terminals, and state directories. Switching is friction. There is no unified control surface.
2. **No secure remote control** — Starting a long-running agent on your desk and checking it from your phone either means (a) SSH + tmux (painful on mobile), (b) closed SaaS (Warp, Cursor Cloud) locking your code in, or (c) exposing your dev machine to the internet.
3. **No session persistence across agents** — Each agent writes logs in its own format in its own directory. There is no single searchable history of "everything my agents did this month."
4. **No policy or isolation** — Agents can read any file, run any command, make any network call. Enterprises and security-minded users have no way to say "this agent can do X but not Y."

Paseo solves #1 and #2 partially (mobile control, multi-provider) but is AGPL-3.0, Node-based, Cloudflare-relay-centric, and has no policy/isolation/interop layer. LeSearch is the Apache-2.0 clean-room answer — a vendor-neutral control plane for any CLI agent, built on open standards.

## 3. Who It's For

### Primary persona — Solo Power-User Developer (Arya)

- Runs Claude Code + Codex + OpenCode daily across multiple projects
- Owns Mac, iPhone, sometimes Linux server, sometimes Raspberry Pi
- Wants to start an agent at desk, check progress from phone, send follow-up tasks from anywhere
- Has zero patience for cloud-dependent tools that touch his code
- Values: open source, privacy, terminal-native, high leverage

### Secondary persona — Security-Conscious Developer

- Works at a company with strict data-egress rules
- Cannot use cloud-hosted AI IDE tooling
- Needs: agent runs locally, identity-based auth, audit trail, tool-call policy enforcement
- Values: zero-trust model, cryptographic audit, no surprise network calls

### Secondary persona — Multi-Provider Hobbyist

- Experiments across LLM providers weekly
- Wants one UI for all CLI agents they try
- Needs: easy provider install, unified timeline, bring-your-own-API-key

### Tertiary persona — OSS Ecosystem Contributor

- Wants to add a new CLI agent to the platform
- Needs: clear AgentProvider spec, sample implementation, CI harness

### Tertiary persona — Small Team Lead (Phase B)

- Hosts one lesearch instance on a server for a 3–10 person team
- Wants per-user identity, per-agent policy, shared workspaces
- Needs: multi-user support (Phase B), audit export to SIEM

### Out of scope (v1)

- Enterprise SSO / LDAP integration (Phase B+)
- Multi-tenant hosted cloud offering (never — self-hosted is the product)
- Windows host support (Linux + macOS first; Windows daemon: Phase B)

## 4. What It Does (user-facing)

> **v0.1.0 ships only the CLI + thin Tauri desktop + responsive web UI on loopback. Two providers (Claude Code + Codex). Loopback transport only. Native iOS/macOS apps and remote dispatch are Phase B.**

### v0.1.0 — From the CLI (loopback only)

```bash
# Install once
brew install lesearch          # or cargo install / curl installer
lesearch daemon start          # binds 127.0.0.1 only

# Spawn a CLI agent (Claude Code or Codex in v0.1.0)
lesearch run --provider claude "summarize this directory"
lesearch run --provider codex --worktree feature-x "add auth middleware"

# Monitor / control
lesearch ls                    # list running + recent agents
lesearch attach <id>           # stream live output
lesearch attach <id> --open-in-$EDITOR   # open in VS Code / Zed / iTerm
lesearch send <id> "also add tests"
lesearch stop <id>

# Search session history (FTS5 substring in v0.1.0; jsongrep later)
lesearch sessions search --grep "TODO"
lesearch sessions replay <id>  # re-prompt (not bit-identical context)

# Health + uninstall
lesearch doctor                # RSS / CPU / FDs / disk / process tree
lesearch uninstall             # verifiable clean teardown
```

### v0.1.0 — Responsive web UI (loopback)

The daemon serves a responsive React UI at `http://127.0.0.1:6767/` (same codebase as the Tauri inner webview). Works in any browser on the host machine. **Loopback only — no remote access in v0.1.0.**

### Phase B — From the CLI (remote dispatch)

```bash
# Phase B only — not in v0.1.0
lesearch dispatch "fix this test" --to claude-on-macbook
```

### v0.1.0 — From the Tauri desktop app (Phase A, loopback only)

- Agent list + live timelines
- Terminal attach (xterm.js embedded)
- Permission prompts surface as notifications
- Session browser + FTS5 substring search bar (jsongrep arrives in v0.2.x)

### Phase B — From the native iOS app (NOT in v0.1.0)

> **Phase B only. Listed here for product roadmap visibility; nothing in this section ships in v0.1.0.**

- QR-code pair with a daemon
- See live agents, approve/deny permission requests
- Voice-dictate follow-up tasks
- Background notifications when an agent completes or asks for permission
- Dispatch a task to any paired daemon

### Phase B — From the native macOS app (NOT in v0.1.0)

> **Phase B only.**

- Menu bar lives with daemon running in background
- Permission sheets for Full Disk Access, Accessibility (one-time)
- CGVirtualDisplay remains a research track; **not** a v0.1.0 or v1.0 ship item
- Integration with Raycast, Alfred, Shortcuts for dispatch shortcuts

### v0.1.0 — From any A2A-speaking tool (read-only)

- Point any A2A client (OpenAI agents ecosystem, Google A2A tools) at `http://127.0.0.1:6767/.well-known/agent-card.json`
- v0.1.0 serves a **read-only** A2A agent card + profile so external tools can discover the daemon
- `POST /message/send` (inbound dispatch from external A2A clients) is **Phase B**
- The daemon targets **A2A 1.0.0** with `A2A-Version` negotiation from day 1

## 5. How It Works (from the user's point of view)

1. **Install** a single binary. Run `lesearch daemon start`. The daemon listens on loopback (`127.0.0.1`) only. Every loopback request carries a per-session bearer token + Origin allowlist (loopback ≠ trust).
2. **Spawn** an agent with `lesearch run`. The daemon creates an isolated per-agent storage namespace via the **StorageBackend trait** — v0.1.0 default is plain directories + OS permissions; AgentFS becomes an opt-in backend when it matures. The daemon loads the appropriate provider adapter (Claude Code or Codex in v0.1.0) and streams output back.
3. **(Phase B)** **Pair** a phone by running `lesearch daemon pair` — a QR code appears. Scan it in the iOS app. Your phone now has a Ziti identity bound to this daemon.
4. **(Phase B)** **Control** from anywhere — your phone on LTE can now reach the daemon through the Ziti overlay, no open ports, no cloud relay required.
5. **Enforce policy** by editing `~/.lesearch/policy.yaml` — an **in-process AVM trait** evaluates every tool call (separate-process sidecar deferred to Phase B). Per-provider `enforcement_mode`: `strict` where pre-execution tool events are reliably interceptable (Claude Code), `audit-only` elsewhere (Codex CLI) with a loud warning at spawn time. Every decision is a signed audit event.
6. **Search history** — every session is a JCS-canonicalized, hash-chained, Ed25519-signed JSONL file. v0.1.0 ships SQLite FTS5 substring search; jsongrep DFA queries arrive in v0.2.x.
7. **Interoperate** — every agent exposes a **read-only** A2A agent card at `/.well-known/agent-card.json` (A2A 1.0.0 with version negotiation). Point any A2A client at your daemon to discover capabilities. Inbound dispatch via `POST /message/send` is Phase B.

## 6. Competitive Landscape

| Product | Open | Self-Host | Multi-Agent | Mobile | Zero-Trust | Policy | Session Search | A2A | License |
|---|---|---|---|---|---|---|---|---|---|
| **LeSearch** (this) | ✅ | ✅ | ✅ (all CLI) | ⚠️ web v0.1.0 / native Phase B | ✅ Ziti (Phase B) | ✅ AVM (in-process) | ✅ FTS5 → jsongrep | ✅ A2A 1.0.0 read-only | Apache-2.0 |
| Paseo | ✅ | ✅ | ✅ (3 agents) | ✅ Expo | ⚠️ ECDH relay | ❌ | ⚠️ basic | ❌ | AGPL-3.0 |
| Warp Dispatch | ❌ | ❌ | ⚠️ own only | ✅ | ❌ | ❌ | ⚠️ | ❌ | Proprietary |
| Cursor / Composer | ❌ | ❌ | ⚠️ own only | ⚠️ | ❌ | ❌ | ⚠️ | ❌ | Proprietary |
| Claude Dispatch | ❌ | ❌ | ❌ Claude only | ⚠️ | ❌ | ❌ | ⚠️ | ❌ | Proprietary |
| OpenFused | ✅ | ✅ | n/a (context only) | ❌ | ⚠️ | ❌ | ⚠️ | ✅ | MIT |
| AgentFS | ✅ | ✅ | n/a (storage only) | ❌ | ❌ | ❌ | n/a | ❌ | MIT |
| Tmux + SSH | ✅ | ✅ | any | ⚠️ painful | ❌ | ❌ | ❌ | ❌ | BSD |
| Claude Code + tmux | ✅ | ✅ | any CLI | ⚠️ painful | ❌ | ❌ | ❌ | ❌ | mixed |

## 7. Differentiation

LeSearch is unique because it is the **only** product that combines:

1. **Open-source + self-hosted** (not SaaS)
2. **Apache-2.0** (not AGPL — commercially safe for downstream)
3. **Multi-provider** (any CLI agent — Claude Code + Codex in v0.1.0; OpenCode/Gemini Phase B)
4. **Pluggable storage backends** (plain directories + OS permissions default; AgentFS opt-in when mature)
5. **Cryptographically tamper-evident session log** (JCS canonical JSON + Ed25519 + hash chain + signed segment manifests)
6. **Session search built-in** (FTS5 substring in v0.1.0; jsongrep DFA in v0.2.x)
7. **A2A facade out of the box** (read-only A2A 1.0.0 agent card in v0.1.0; inbound dispatch Phase B)
8. **In-process AVM policy engine** (per-provider `enforcement_mode: strict | audit-only`; separate-process sidecar Phase B)
9. **Hardened localhost** (per-session bearer token + Origin allowlist + OS-auth on sensitive ops)
10. **Phase B roadmap**: zero-trust remote (OpenZiti), native macOS + iOS (Swift), inbound A2A dispatch

> **Phase B research track (NOT a v0.1.0 ship item)**: per-agent CGVirtualDisplay isolation on macOS. Tracked in the cmux salvage branch — not relied on by the product.

None of the existing products combine more than 3 of items 1–9. This is the moat.

## 8. Positioning Statement

> For developers who run AI coding agents and demand open, self-hosted, privacy-first infrastructure, **LeSearch** is the **open agent control plane** that combines multi-provider agent orchestration, pluggable per-agent storage isolation, cryptographically tamper-evident session history with built-in search, and a vendor-neutral A2A 1.0.0 facade — without cloud dependencies or vendor lock-in. Phase B adds zero-trust remote access (OpenZiti) and native macOS + iOS apps. Unlike Paseo (AGPL, Expo, no policy, no search), Cursor Cloud (closed, SaaS), or raw tmux+SSH, LeSearch is Apache-2.0, speaks the open A2A standard natively, and treats every loopback request as untrusted by default (bearer token + Origin allowlist + OS-auth on sensitive ops).

## 9. Success Indicators

> **Release gate** (hard, engineering-measurable) lives in `PRD.md §8.1`. The bullets below are **signals**, not gates — useful to track but never block a release.

Phase A signals (dogfood; not release gate):
- Arya prefers `lesearch` over direct `claude` / `codex` for daily work after the 5 hard engineering criteria pass (PRD §8.1)
- Paseo users + OpenFused users on Discord notice and try the Apache-2.0 alternative
- At least one external contributor adds an agent provider within 60 days of public launch

Phase B signals (native apps + remote):
- Arya controls agents from phone on LTE for real work at least 5 times per week
- First enterprise / security team tries it as a self-hosted alternative to SaaS dev-AI tools

## 10. Out of Scope (v0.1.0)

- Cloud-hosted multi-tenant LeSearch
- Windows daemon (Phase B if demand)
- Proprietary agent providers (closed-source CLI wrapping)
- SSO / SAML / LDAP (Phase B)
- Code editor embed — `lesearch attach --open-in-$EDITOR` is the only IDE affordance in v0.1.0; full plugin is not on the roadmap
- **AgentFS as hard dependency** — pluggable backend only; plain dirs default
- **CGVirtualDisplay** — research track only; not a v0.1.0 or v1.0 ship item
- **Six-repo / 19-crate split** — single monorepo with 5–7 crates
- **Full AVM enforcement on opaque CLIs** — strict for interceptable providers; audit-only with loud warning elsewhere
- **iOS + native macOS apps** — Phase B
- **OpenZiti + Noise-WS transports** — Phase B (v0.1.0 = loopback only)
- **Inbound A2A dispatch** (`POST /message/send`) — Phase B (v0.1.0 = read-only card)
- **Generic-a2a provider import** — Phase B
- **Push approvals, voice dictation, remote desktop** — Phase B
- **OTEL by default** — opt-in feature flag only
- **CBOR session export** — markdown + jsonl only in v0.1.0
- **Bit-identical session replay** — v0.1.0 re-prompts with original prompt + cwd; opaque CLI auth + cached context + model versions are not recreated

## 11. Open Positioning Question (needs user decision)

Two candidate public names:

- `lesearch` — short, unique, consistent with LeSearch AI brand
- `agentctl` or `agents` or `mconnect` — generic, searchable

Recommendation: stay with `lesearch` — it's part of the personal brand, and the product rides the LeSearch AI umbrella.

Also TBD: whether to publicly acknowledge the "former LeCoder MConnect" heritage or present LeSearch as a fresh product.

## 12. Revision History

| Rev | Date | Notes |
|---|---|---|
| 0.1 | 2026-04-14 | Initial draft for multi-agent review. |
| 0.2 | 2026-04-14 | Review synthesis applied (A1–A12, D1–D5). Positioning rebrand to "Open Agent Control Plane". v0.1.0 scope locked: loopback only, 2 providers, plain-dir storage, in-process AVM, read-only A2A 1.0.0. CGVirtualDisplay → research track. Native iOS/macOS + remote → Phase B. |

