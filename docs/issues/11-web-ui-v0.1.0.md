# Issue: FR-42 responsive web UI on loopback — v0.1.0 mobile story

**Labels:** `area/web` `area/frontend` `type/feature` `priority/high` `release-gate/v0.1.0` `phase/A.6`
**Milestone:** v0.1.0 Daily Driver
**FR refs:** FR-42 (PRD rev-0.2)
**Story refs:** S-4.0 (rev-0.2 D1)
**Blocked by:** PR #3 (v0.1.0 scaffold), issue #06 (bearer auth)

## Summary

Rev-0.2 D1 designates a responsive web UI served on loopback as the v0.1.0 mobile story — mobile browsers hitting `http://127.0.0.1:6767/` (or via Tauri wrapper) get the same experience as desktop. No native iOS/macOS apps in v0.1.0. Minimal functional scope: agent list, live terminal view, session search.

## Acceptance criteria

- [ ] New directory `apps/web/` (Vite + React + TypeScript + Tailwind)
- [ ] Daemon serves built assets from `apps/web/dist/` at `/` via `ServeDir`
- [ ] Routes:
  - `/` — agent list (polls `agent.list` every 2s)
  - `/agents/:id` — live terminal view (WebSocket attach, renders stdout stream)
  - `/sessions` — session search (calls `session.search`)
  - `/doctor` — daemon health + version + active backend (calls `/health` + derived info)
- [ ] Responsive breakpoints: mobile portrait (360px), tablet (768px), desktop (1024px+)
- [ ] Terminal view uses `xterm.js` for rendering — passes ANSI escapes through
- [ ] Bearer token: web UI reads token from URL fragment on first load (`#token=...`) or prompts user; stores in `sessionStorage` (not localStorage — scoped to tab lifetime)
- [ ] Agent spawn button sends `agent.spawn` with a prompt from a textarea
- [ ] Build pipeline: `bun install && bun run build` produces `apps/web/dist/`; committed under `.gitignore`
- [ ] Daemon `build.rs` runs `bun run build` during `cargo build` (behind `--features web` to keep CI fast)
- [ ] Playwright smoke tests in `apps/web/tests/` — 5 happy-path flows
- [ ] Accessibility: WCAG AA — keyboard navigation, aria labels, color-contrast check in CI

## Non-goals

- Styling polish beyond Tailwind defaults (ship functional, iterate on design later)
- User accounts / multi-user (Phase B)
- Dark mode toggle (use `prefers-color-scheme` media query only)
- PWA / offline (Phase B)
- Native file upload (Phase B)

## Implementation notes

- Stack: Vite + React 18 + TypeScript + Tailwind + TanStack Router + `xterm.js`
- WebSocket wrapper: tiny client in `apps/web/src/lib/ws.ts` mirrors `lesearch-cli/src/ws_client.rs` API shape
- Bearer token UX: on first load, if no token in fragment, show a one-time "paste your token" screen with link to `lesearch daemon show-token` CLI command
- Server-side: `axum` `ServeDir` + `ServeFile` fallback to `index.html` for SPA routing
- Build flag: `cargo build --features web` triggers `build.rs` to invoke `bun run build`

## References

- `docs/PRD.md` rev-0.2 §FR-42
- `docs/EPICS_AND_STORIES.md` §E4 (rev-0.2 S-4.0)
- `docs/REVIEW_SYNTHESIS.md` §D1
- [xterm.js](https://xtermjs.org/)
- [TanStack Router](https://tanstack.com/router/)
- [Vite](https://vitejs.dev/)
