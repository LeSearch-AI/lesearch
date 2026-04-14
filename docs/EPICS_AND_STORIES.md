# LeSearch — Epics & User Stories

**Version**: 0.2 (review synthesis applied — A1–A12, D1–D5)
**Date**: 2026-04-14
**Status**: Backlog ruthlessly cut to v0.1.0 scope; rest deferred to v0.2.0 / Phase B.

Format: each epic carries a JTBD one-liner (per D4) and BDD-style acceptance criteria for test authors.

---

## E1 — Installation & Onboarding

**JTBD**: When I try a new agent control plane, I want to install and uninstall it cleanly in under 5 minutes, so that I can evaluate it without risk to my machine.

### S-1.1 — One-command install
**As a** solo developer, **I want to** install LeSearch with one command, **so that** I can try it immediately without reading setup docs.

**Acceptance**
- GIVEN a macOS or Linux machine with Homebrew (or cargo) installed
- WHEN I run `brew install lesearch` (or `cargo install lesearch`)
- THEN a single `lesearch` binary is on my PATH within 60 seconds
- AND `lesearch --help` prints usage

### S-1.2 — First-run guided setup
**As a** new user, **I want** `lesearch` to detect my providers and pick safe defaults on first-run, **so that** I don't have to read config docs upfront.

**Acceptance**
- WHEN I run `lesearch daemon start` for the first time
- THEN the daemon silently selects **"Local Only"** (loopback `127.0.0.1`) — no transport choice presented in v0.1.0; protocol terminology (`direct`/`ziti`/`noise-ws`) MUST NOT appear in installer copy (FR-19)
- AND providers are detected by scanning `PATH` for `claude` and `codex` (v0.1.0 supported set; OpenCode + Gemini CLI become detectable in Phase B)
- AND a `~/.lesearch/config.toml` is created with detected providers, an empty `origin_allowlist` populated with `http://127.0.0.1:6767`, and `[auth]` block defaults
- AND **(Phase B)** when "Remote Access" mode is added, first-run gains an additional toggle — never a raw transport-name picker

### S-1.3 — Clean uninstall
**As a** careful user, **I want to** uninstall LeSearch with one command, **so that** I have confidence it leaves no trace.

**Acceptance**
- WHEN I run `lesearch uninstall`
- THEN all daemon processes are stopped
- AND all AgentFS mounts are unmounted
- AND all LaunchAgents / systemd units are removed
- AND config, caches, session data, AgentFS files are deleted (with `--keep-sessions` opt-out)
- AND the binary removes itself last

### S-1.4 — Visible permission requests
**As a** security-conscious user, **I want** LeSearch to ask explicitly before requesting OS permissions (Full Disk Access, NFS mount, Accessibility), **so that** I understand what I'm granting.

**Acceptance**
- BEFORE any permission prompt appears
- THEN a LeSearch-native dialog explains: which permission, why, what happens if denied
- AND I can deny and still use the degraded feature set

---

## E2 — Spawn & Control Agents

**JTBD**: When I'm coding across multiple projects, I want one command surface to spawn and steer any CLI agent, so that I stop context-switching between tools.

### S-2.1 — Spawn any CLI agent
**As a** developer, **I want to** spawn Claude Code, Codex, OpenCode, or Gemini CLI through a single command, **so that** I don't context-switch between tools.

**Acceptance**
- GIVEN provider is installed on the host
- WHEN I run `lesearch run --provider claude "hello"`
- THEN an agent starts, emits output streamed to stdout
- AND an agent ID is printed for later reference

### S-2.2 — List active and recent agents
**As a** multi-project developer, **I want to** see my agents at a glance, **so that** I know what's running where.

**Acceptance**
- WHEN I run `lesearch ls`
- THEN a table shows: ID, provider, cwd (truncated), status, started, runtime
- AND `lesearch ls -a` adds recently-finished agents
- AND `lesearch ls --project <dir>` filters to agents in that cwd

### S-2.3 — Attach to live output
**As a** developer, **I want to** attach to a running agent's output stream, **so that** I can observe progress.

**Acceptance**
- WHEN I run `lesearch attach <id>`
- THEN I see a replay of the session buffer, then live output
- AND `Ctrl-c` detaches cleanly without killing the agent

### S-2.4 — Send follow-up prompt
**As a** developer, **I want to** send an additional message to a running or idle agent, **so that** I can steer it iteratively.

**Acceptance**
- WHEN I run `lesearch send <id> "also add tests"`
- THEN the message reaches the agent on its next turn boundary
- AND the new prompt appears in the session log

### S-2.5 — Stop cleanly
**As a** developer, **I want to** stop an agent and know no zombie processes remain, **so that** my machine stays tidy.

**Acceptance**
- WHEN I run `lesearch stop <id>`
- THEN the agent receives SIGTERM with 5s grace, then SIGKILL
- AND within 6s the agent appears as `status=stopped` in `lesearch ls`
- AND no child processes remain (`pgrep -P <pid>` is empty)

### S-2.6 — Concurrent agent ceiling
**As a** self-hoster, **I want to** limit how many agents run concurrently, **so that** my machine doesn't thrash.

**Acceptance**
- GIVEN `max_concurrent_agents = 3` in config
- WHEN I try to spawn a 4th agent
- THEN the command errors with a clear message + suggestion (stop one or raise the cap)

---

## E3 — Persistent Sessions

**JTBD**: When an agent did something interesting last week, I want to search for it across every session, so that I can recall, verify, or replay it.

### S-3.1 — Session log auto-capture
**As a** developer, **I want** every agent session logged automatically, **so that** I can review anything later without opting in.

**Acceptance**
- WHEN any agent starts
- THEN a JSONL file is created at `$LESEARCH_HOME/agents/{id}/sessions/{session-id}.jsonl`
- AND every event (stream chunk, tool call, permission decision, exit) is appended as one Ed25519-signed CloudEvent line
- AND the file is never mutated after writing (append-only)

### S-3.2 — Search across all sessions
**As a** power user, **I want to** search across all past sessions for specific events or text, **so that** I can recall what an agent did.

**Acceptance**
- WHEN I run `lesearch sessions search "**.tool_call.name == \"Bash\""`
- THEN jsongrep DFA matches over all session JSONL return matching events with path + timestamp
- AND substring-style search `lesearch sessions search --grep "TODO"` uses FTS5 for < 100ms over 1 GB

### S-3.3 — Verify session integrity
**As a** compliance-conscious user, **I want to** verify a session file wasn't tampered with, **so that** I can rely on it as audit evidence.

**Acceptance**
- WHEN I run `lesearch sessions verify <id>`
- THEN every Ed25519 signature is checked against the daemon's recorded public key
- AND the command exits 0 iff all events are signed by the expected key and the log is append-only
- AND tampered files report the first tampered line number

### S-3.4 — Replay a session
**As a** developer, **I want to** replay a prior session, **so that** I can continue work with the same context without manually reconstructing.

**Acceptance**
- WHEN I run `lesearch sessions replay <id>`
- THEN a new agent is spawned with the original provider, prompt, cwd, environment, and last-known state
- AND the new agent receives the original system prompt PLUS a "replay context" message

### S-3.5 — Export a session
**As a** content creator, **I want to** export a session as markdown, **so that** I can turn it into a blog post or review.

**Acceptance**
- WHEN I run `lesearch sessions export <id> --format markdown`
- THEN a `.md` file is produced with event stream, tool calls formatted as code blocks, timestamps
- AND `--format jsonl` also works in v0.1.0; `--format cbor` is **Phase B** (per ruthless v0.1.0 scope cut)

### S-3.6 — Session retention and rotation
**As a** self-hoster, **I want** old sessions to be archived/pruned automatically, **so that** my disk doesn't fill up.

**Acceptance**
- GIVEN config `retention_days = 90`
- WHEN a session is older than 90 days
- THEN it's compressed to `.jsonl.zst` (retaining verifiability)
- AND after `retention_days + 180` it's deleted (configurable)

---

## E4 — Multi-Device Control

**JTBD**: When I'm away from my desk, I want to monitor and steer agents from another device, so that long-running work doesn't pin me to one machine.

> **Phase B — NOT in v0.1.0** (except S-4.0 below). v0.1.0 ships only the loopback responsive web UI; native iOS/macOS, remote pairing, push notifications, and voice dictation are Phase B.

### S-4.0 — Responsive web UI on loopback (v0.1.0)

**As a** developer working from any browser on my host machine, **I want to** open a responsive web UI served by the daemon, **so that** I can monitor agents from a tab without installing the desktop app.

**Acceptance**
- WHEN the daemon is running and I open `http://127.0.0.1:6767/`
- THEN a responsive React UI loads (same codebase as the Tauri inner webview)
- AND it shows the same agent list + live timelines + session search as the Tauri shell
- AND the daemon refuses requests from any non-loopback Origin (default deny)
- AND every request carries a per-session bearer token (FR-40)

### S-4.1 — Pair a phone via QR code (Phase B)
**As a** developer, **I want to** pair my iPhone with my local daemon by scanning a QR code, **so that** I don't need to type secrets.

**Acceptance**
- WHEN I run `lesearch daemon pair` on the daemon host
- THEN a QR code appears (scannable in terminal as ANSI or browser)
- AND scanning from the iOS app enrolls the phone's identity in the daemon keyring
- AND the first reachability test from the phone succeeds

### S-4.2 — Phone sees agents in real time
**As a** mobile user, **I want** the iOS app to show my agents' live timelines, **so that** I can monitor work while away from my desk.

**Acceptance**
- WHEN an agent on the paired daemon updates its timeline
- THEN the iOS app receives the update within 500 ms p99 on good LTE
- AND the UI renders the new events without requiring refresh

### S-4.3 — Approve a tool permission from phone
**As a** mobile user, **I want** to get a push notification when an agent needs permission for a risky tool call, and approve/deny inline, **so that** I don't block agent progress while away from my desk.

**Acceptance**
- GIVEN an agent requests permission for a tool requiring consent
- WHEN the iOS app is backgrounded
- THEN a push notification arrives within 10 s
- AND I can tap `Approve` / `Deny` from the notification without opening the app
- AND the daemon receives the decision and resumes

### S-4.4 — Dispatch from phone
**As a** mobile user, **I want** to dispatch a task from my phone to any paired daemon, **so that** I can assign work from anywhere.

**Acceptance**
- WHEN I enter "write me a script to…" in the iOS composer
- AND select `claude-on-macbook` as target
- THEN a new agent is spawned on that daemon with my prompt
- AND I see streaming output in the iOS app

### S-4.5 — Voice dictation
**As a** mobile user, **I want** to dictate follow-up prompts with my voice, **so that** I can use the app hands-free.

**Acceptance**
- WHEN I tap-and-hold the mic in the composer
- THEN speech is transcribed locally (Apple Speech framework on iOS) or optionally via a server model
- AND the transcript is editable before sending

---

## E5 — Zero-Trust Transport

**JTBD**: When I want remote control of my agents, I want identity-based zero-trust transport, so that I never expose dev hardware to the internet.

> **Phase B only — NOT in v0.1.0.** v0.1.0 ships loopback-only ("Local Only" UX). OpenZiti and Noise-WS arrive in Phase B as a "Remote Access" toggle.

### S-5.1 — Transport selectable in config (Phase B)
**As a** security-conscious user, **I want** to pick my transport mode (direct / Ziti / Noise-WS), **so that** I match my threat model.

**Acceptance**
- GIVEN `transport = "ziti"` in config
- WHEN the daemon starts
- THEN it does NOT bind on any TCP/UDP port
- AND only Ziti-enrolled clients can reach it
- AND `nmap` against the host shows zero relevant listening ports

### S-5.2 — Self-host Ziti controller
**As a** self-hoster, **I want** clear docs for running my own Ziti controller in Docker, **so that** I don't rely on a third-party controller.

**Acceptance**
- GIVEN `docs/ZITI_SETUP.md`
- WHEN I follow the steps
- THEN a Ziti controller runs locally in Docker within 15 minutes
- AND `lesearch daemon pair --issuer http://localhost:1280` produces a working enrollment JWT

### S-5.3 — Noise-WS fallback for zero-config
**As a** newcomer, **I want** a fallback transport that doesn't require standing up Ziti, **so that** I can get remote control working in 5 minutes.

**Acceptance**
- GIVEN `transport = "noise-ws"` selected
- WHEN I run `lesearch daemon expose` (with explicit confirmation)
- THEN a WebSocket endpoint is exposed through a configurable remote relay (paseo's, zrok's, or user's)
- AND the relay sees only ciphertext
- AND CLI clearly states: "Noise-WS is defense-in-depth, not zero-trust; prefer Ziti for production"

---

## E6 — Policy & Safety (AVM)

**JTBD**: When an agent runs autonomously, I want a policy engine to block obvious footguns and audit every tool call, so that I can trust it without supervising every command.

> **v0.1.0**: in-process AVM trait + baseline deny-list (S-6.1) only. Strict enforcement runs for Claude Code (interceptable MCP); Codex CLI runs in `audit-only` mode with a loud spawn-time warning. Per-agent policies, hot reload, and per-provider strict enforcement everywhere are Phase B.

### S-6.1 — Baseline policy blocks dangerous commands
**As a** user, **I want** a sensible default policy blocking obvious footguns, **so that** an agent can't destroy my system on day one.

**Acceptance**
- GIVEN default policy loaded
- WHEN an agent attempts `rm -rf /`, `curl … | sh`, or write to `/etc/*`
- THEN the AVM returns `deny`
- AND the decision is logged as a signed event
- AND the agent sees an error explaining the deny + how to allowlist

### S-6.2 — Edit policy without restart
**As a** power user, **I want** to edit `policy.yaml` and reload without restarting the daemon, **so that** I iterate fast.

**Acceptance**
- WHEN I edit `~/.lesearch/policy.yaml`
- AND run `lesearch policy reload`
- THEN future tool calls use the new policy
- AND in-flight tool calls continue with the old policy (no mid-call swap)

### S-6.3 — Audit trail verifiable
**As a** auditor, **I want** to verify that the recorded policy decisions were signed by the daemon, **so that** I can trust them in compliance review.

**Acceptance**
- WHEN I run `lesearch audit verify <file.jsonl>`
- THEN every entry's Ed25519 signature is checked
- AND mismatch is reported with line numbers
- AND exit code reflects pass/fail

### S-6.4 — Per-agent policy
**As a** power user, **I want** different agents to run under different policies, **so that** a sandboxed task agent has tighter limits than a personal assistant.

**Acceptance**
- WHEN I spawn `lesearch run --provider claude --policy restricted "…"`
- THEN the `restricted` policy is loaded for this agent
- AND tool calls are evaluated against that policy

---

## E7 — Storage & Isolation (StorageBackend trait)

**JTBD**: When I run multiple agents on one machine, I want each agent to only see its own working directory, so that a compromised agent cannot exfiltrate cross-agent data.

> **v0.1.0**: pluggable `StorageBackend` trait. Default backend = **plain directories + OS permissions**. AgentFS is a feature-flagged opt-in backend (`--features agentfs`) when the upstream SDK reaches production stability.

### S-7.1 — Per-agent storage namespace
**As a** security-conscious user, **I want** each agent to only see its own files, **so that** a compromised agent cannot exfiltrate cross-agent data.

**Acceptance**
- WHEN two agents A and B are spawned with the default plain-dir backend
- THEN agent A's namespace root is `/Users/$USER/.lesearch/agents/A/fs/` with `chmod 700` owned by the daemon's effective user
- AND any read attempt from inside A's process to `/Users/$USER/.lesearch/agents/B/fs/` returns `EACCES`
- AND verified via integration test that asserts both the permission bits and the cross-agent denial
- AND the same test re-runs against the AgentFS backend when `--features agentfs` is enabled (Phase B / opt-in)

### S-7.2 — Shared workspace
**As a** multi-agent orchestrator, **I want** agents to share a workspace when I opt in, **so that** they can collaborate on a single codebase.

**Acceptance**
- WHEN I run `lesearch workspace create my-project --agents claude-1,codex-2`
- THEN both agents are exposed to the same storage namespace via the active backend (shared directory under `PlainDirBackend`; shared mount under opt-in `AgentFsBackend`) — see FR-12
- AND writes from one are visible to the other within 100 ms
- AND the workspace auto-closes when the last agent exits (configurable)

### S-7.3 — Storage footprint visible
**As a** self-hoster, **I want** to see how much disk each agent is using, **so that** I can manage disk pressure.

**Acceptance**
- WHEN I run `lesearch storage status`
- THEN per-agent and shared-workspace disk usage are reported
- AND session log size is separated from AgentFS size
- AND pruning candidates (old sessions, finished agents) are listed

---

## E8 — A2A Interop

**JTBD**: When I use external A2A-speaking tools, I want them to discover and (eventually) drive my local agents, so that I'm not locked into LeSearch's UI.

> **v0.1.0**: read-only A2A 1.0.0 agent card + profile + `A2A-Version` negotiation. Inbound dispatch (`POST /message/send`), `generic-a2a` provider import, and OpenFused inbox are **Phase B**.

### S-8.1 — External A2A client consumes our agent (Phase B)
**As a** user of Google's A2A-compatible tools, **I want** to drive my local lesearch agent from those tools, **so that** I don't have to maintain two agent setups.

**Acceptance**
- GIVEN daemon running with A2A enabled + bearer token configured
- WHEN an external A2A client hits `/.well-known/agent-card.json` and `POST /message/send`
- THEN a task is created, SSE progress streams back, and the final result is available at `/tasks/{id}`

### S-8.2 — Import an external A2A agent as a provider
**As a** orchestrator, **I want** to dispatch tasks from lesearch to an external A2A agent, **so that** my dispatch layer reaches beyond my local CLI agents.

**Acceptance**
- WHEN I add an external A2A endpoint via `lesearch provider add a2a https://peer.example/agent-card.json`
- THEN the external agent appears as a provider in `lesearch providers ls`
- AND `lesearch run --provider peer.example "…"` dispatches via A2A

### S-8.3 — OpenFused interop
**As a** OpenFused user, **I want** to send signed messages to my lesearch agent from `openfuse send`, **so that** the two ecosystems overlap.

**Acceptance**
- GIVEN OpenFused CLI installed, lesearch daemon running with inbox enabled
- WHEN I run `openfuse send my-lesearch-agent "…"`
- THEN the message lands in the lesearch agent's inbox
- AND is tagged `[VERIFIED]` if the signing key is in the daemon keyring

---

## E9 — Native macOS & iOS (Phase B)

**JTBD**: When I want a polished native experience, I want first-class macOS + iOS apps, so that LeSearch fits the platform I work on.

> **Phase B only — NOT in v0.1.0.** v0.1.0 ships only the Tauri desktop + responsive web UI on loopback.

### S-9.1 — macOS app ships the daemon
**As a** casual macOS user, **I want** a one-click installer for a macOS app that runs the daemon in the background, **so that** I don't need to use the CLI.

**Acceptance**
- WHEN I install and open `LeSearchMac.app`
- THEN the daemon starts as a LaunchAgent signed with the app's Team ID
- AND the menu bar shows agent status
- AND closing the UI doesn't stop the daemon (keeps running in background)

### S-9.2 — Per-agent virtual display (macOS host) — RESEARCH TRACK ONLY

> **NOT a v0.1.0 or v1.0 ship item.** Tracked in the cmux salvage branch as a research experiment. Apple's CGVirtualDisplay is private API, breaks across macOS versions, and is App Store hostile. Re-evaluated only if a stable public alternative appears.

**As a** power user on macOS, **I want** each GUI-capable agent to run in its own virtual display, **so that** I can observe multiple agents doing visual work simultaneously.

**Acceptance**
- GIVEN macOS 13+, app granted necessary permissions
- WHEN I spawn an agent with `--visual`
- THEN a virtual monitor is created via CGVirtualDisplay
- AND I can view it via the embedded VNC panel in the app
- AND pointer events are grounded with `[POINT:x,y]` CLI overlays

### S-9.3 — iOS background daemon awareness
**As a** iOS user, **I want** the app to receive push notifications when an agent needs attention, **so that** I don't miss important events.

**Acceptance**
- GIVEN app installed, push permission granted
- WHEN an agent requires permission or completes
- THEN a silent push wakes the app (or user push if permitted)
- AND an actionable notification appears

---

## E10 — Remote Desktop (Phase B)

**JTBD**: When an agent needs a GUI on a remote host, I want to see and steer that GUI from any browser, so that headless servers stay useful for visual tasks.

> **Phase B only — NOT in v0.1.0.**

### S-10.1 — HTML5 VNC for Linux hosts
**As a** Linux host user, **I want** to see a GUI agent's display from any browser, **so that** I don't need a VNC client.

**Acceptance**
- GIVEN KasmVNC running on the Linux host, wrapped as a Ziti service
- WHEN I open the web console or iOS app
- THEN the agent's display streams in a browser-native view (no extra client install)

---

## E11 — Observability

**JTBD**: When something goes wrong, I want a single command and a single dashboard to tell me what's wrong, so that I can fix it without reading raw logs for an hour.

> **v0.1.0**: `lesearch doctor` (S-11.3) + agents-observe hook events (S-11.2). OTEL is **opt-in only** — see S-11.1.

### S-11.1 — OpenTelemetry traces (opt-in)
**As an** operator, **I want** the daemon to emit OTEL spans to a local collector when I enable it, **so that** I can diagnose issues without paying the cost when I don't need it.

**Acceptance**
- GIVEN `otel.endpoint` is **empty** in default config (off by default)
- WHEN I set `otel.endpoint = "http://localhost:4318"` and restart
- THEN spans for `agent.create`, `tool.call`, `policy.decision`, `session.write` are emitted
- AND context propagates across provider and the in-process AVM trait
- AND with the default empty endpoint, **zero** OTEL machinery runs (no background exporter task)

### S-11.2 — agents-observe integration
**As a** hook-based observer user, **I want** lesearch to emit the same hook events as Claude Code, **so that** agents-observe dashboard works out of the box.

**Acceptance**
- GIVEN agents-observe server running
- WHEN lesearch spawns an agent
- THEN PreToolUse / PostToolUse / SessionStart / Stop events are POSTed to agents-observe
- AND appear in the dashboard

### S-11.3 — `lesearch doctor`
**As a** self-hoster, **I want** a single diagnostic command, **so that** I can answer "is it healthy?" fast.

**Acceptance**
- WHEN I run `lesearch doctor`
- THEN RAM / CPU / open FDs / mount points / CI status / transport status / disk usage report in < 2 s
- AND problems are highlighted with remediation hints

---

## E12 — Resource Hygiene (user-flagged critical)

**JTBD**: When LeSearch runs in the background on my dev machine, I want it to consume bounded resources, so that it never competes with my actual work.

### S-12.1 — Memory ceiling
**As a** self-hoster, **I want** the daemon to respect a hard memory ceiling, **so that** it can't eat my entire RAM.

**Acceptance**
- GIVEN `max_daemon_memory_mb = 500` in config
- WHEN daemon usage hits the ceiling
- THEN new agent creation is refused with `resource_exhausted` error
- AND `lesearch doctor` flags the condition
- AND on macOS/Linux cgroups are used where available

### S-12.2 — No background polling when idle
**As a** power user, **I want** the daemon to sleep when no clients are connected, **so that** it doesn't cost CPU.

**Acceptance**
- GIVEN no clients connected for > 60 s
- THEN background timers are suspended or elongated to ≥ 30 s intervals
- AND CPU usage averages ≤ 0.05% over 5 minutes

### S-12.3 — Agents leave no orphans
**As a** careful user, **I want** every agent's subprocess tree to terminate when the agent terminates, **so that** I don't accumulate zombies.

**Acceptance**
- WHEN an agent exits or is killed
- THEN all child processes spawned by the agent (provider CLI + any children) are reaped within 2 s
- AND verified by integration test using process-tree inspection

### S-12.4 — Disk usage transparent
**As a** self-hoster, **I want** to see disk impact live, **so that** I don't wake up to a full disk.

**Acceptance**
- WHEN I run `lesearch storage status`
- THEN total disk used by lesearch is shown with top 10 offenders
- AND a warning fires if total exceeds `storage_warn_gb`

### S-12.5 — No phone-home
**As a** privacy-conscious user, **I want** zero outbound network calls by default, **so that** I can audit easily.

**Acceptance**
- GIVEN fresh install, no user action taken
- WHEN I inspect network traffic for 24 h
- THEN no outbound connection occurs except to targets I explicitly configured (pair peers, A2A endpoints, upstream provider APIs the agents themselves call)

### S-12.6 — Opt-in auto-update
**As a** cautious user, **I want** updates to require my explicit consent, **so that** a compromised update stream can't push code to my machine silently.

**Acceptance**
- GIVEN default install
- WHEN a new release exists on GitHub
- THEN the daemon does NOT auto-upgrade
- AND `lesearch update` prompts me before any download

---

## E13 — Developer Experience

**JTBD**: When I extend LeSearch — adding a provider, building a UI client, or hopping back into a session from my editor — I want the seams to be obvious and small, so that I stay in flow.

### S-13.1 — Add a provider in ≤ 200 LOC
**As a** OSS contributor, **I want** to add a new agent provider with minimal code, **so that** community-driven integrations are viable.

**Acceptance**
- GIVEN `docs/AGENT_PROVIDER.md` reference
- WHEN I implement the `AgentProvider` trait
- THEN my provider integrates in < 200 lines (80/20) and < 2 hours on a typical CLI

### S-13.2 — SDK for third-party UI clients (Phase B)
**As a** third-party UI builder, **I want** a stable client SDK, **so that** I can build alternate front-ends without reverse-engineering.

**Acceptance**
- GIVEN published Rust + TS SDK
- WHEN I read `docs/CLIENT_SDK.md`
- THEN connecting, subscribing to timelines, and issuing commands takes ≤ 50 lines of code

### S-13.3 — Attach in my editor (`--open-in-$EDITOR`) (v0.1.0)

**As a** developer who lives in VS Code / Zed / iTerm, **I want** `lesearch attach <id> --open-in-$EDITOR` to open the attached terminal in my preferred editor, **so that** I never have to leave my IDE to babysit a running agent.

**Acceptance**
- GIVEN `$EDITOR=code` is set in the shell environment
- WHEN I run `lesearch attach <id> --open-in-$EDITOR`
- THEN the attached terminal opens in a VS Code integrated terminal pane
- AND `--open-in-zed` / `--open-in-iterm` work analogously when those tools are on PATH
- AND if the editor is not detected, the command falls back to a normal foreground attach with a one-line warning
- AND no full IDE plugin is installed or required

---

## Prioritization (ruthless v0.1.0 scope)

**Must-have for v0.1.0** (5 hard engineering gates per `PRD.md §8.1`):
- E1 (install + uninstall) — full
- E2 (spawn + control, 2 providers: Claude Code + Codex) — full
- E3 (session capture + JCS-canonical + hash-chain + Ed25519 + FTS5 substring search; jsongrep deferred) — full
- E4.0 (responsive web UI on loopback) — only S-4.0 from E4 ships in v0.1.0
- E6 (in-process AVM trait + baseline deny-list; per-provider `enforcement_mode`; Codex = audit-only) — partial
- E7 (storage + isolation via `StorageBackend` trait, plain-dirs default) — full
- E8 (read-only A2A 1.0.0 agent card + `A2A-Version` negotiation) — partial
- E11.2 (agents-observe hook events) + E11.3 (`lesearch doctor`); E11.1 OTEL **opt-in only**
- E12 (resource hygiene — full)
- E13.1 (provider API) + E13.3 (`--open-in-$EDITOR`)

**Deferred to v0.2.0 / Phase B**:
- E4 native iOS / push / voice / dispatch — Phase B
- E5 (OpenZiti + Noise-WS transports) — Phase B
- E6 hot-reload + per-agent policy + strict everywhere — Phase B
- E8 inbound dispatch (`POST /message/send`) + generic-a2a import + OpenFused inbox — Phase B
- E9 native macOS + iOS apps — Phase B (S-9.2 CGVirtualDisplay = research track only, never on the v0.1.0 / v1.0 roadmap)
- E10 remote desktop — Phase B
- E11.1 OTEL on by default — never default
- E13.2 public SDK polish — Phase B

---

## Revision History

| Rev | Date | Notes |
|---|---|---|
| 0.1 | 2026-04-14 | Initial backlog for review. |
| 0.2 | 2026-04-14 | Review synthesis applied (A1–A12, D1–D5). JTBD added per epic. E4/E5/E9/E10 → Phase B banners. New S-4.0 (responsive web UI on loopback). New S-13.3 (`--open-in-$EDITOR`). E7.1 → plain-dirs backend default. E8 → read-only A2A 1.0.0 in v0.1.0. E11.1 OTEL → opt-in. S-9.2 CGVirtualDisplay → research track only. Prioritization rewritten against ruthless v0.1.0 scope. |

