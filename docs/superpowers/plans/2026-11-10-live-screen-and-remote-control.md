# Live Screen and Remote Control Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make an accepted SparkDesk session show the host's live Windows screen and allow bounded remote input only after a separate host-side approval.

**Architecture:** Keep Tauri/React as the desktop shell and Rust as the trust boundary. Add named local identity profiles, authenticate the peer over the existing DTLS-protected data channels with a signed challenge, send bounded screen frames, validate and gate normalized mouse/keyboard events in Rust, and expose a bilingual live-view/control UI.

**Tech Stack:** Rust 2024, Tauri 2, existing `webrtc` 0.21.1, Windows capture and `SendInput`, React/Vite, existing Radix Themes and Lucide components.

**Spec:** `docs/superpowers/specs/2026-11-10-live-screen-and-remote-control-design.md`

## Global Constraints

- Device ID locates a device; it does not grant access.
- Accept every incoming session locally; remote input requires a second explicit local approval and is off by default.
- Verify peer identity before sharing frames or accepting input.
- Bound frames and input payloads; keep only the newest frame pending.
- Release held synthetic input and stop capture at revoke/disconnect/error.
- Local profile names must not escape the SparkDesk config directory or overwrite the default key.
- Windows API input does not bypass UIPI, secure desktops or elevation boundaries.

## Review Focus

- A peer with a false device ID or invalid session signature: no frame or input capability is granted.
- A control message before host approval or after revoke: no OS event occurs.
- Malformed/oversized frames or input, unknown keys, coordinate edges and event floods: rejected without panic or unbounded allocation.
- Disconnect during a held key/button: all held synthetic inputs are released and capture stops.
- Invalid profile names and two processes with the same profile: no path traversal or silent key reuse/corruption.

### Task 1: Separate local device profiles

**Files:** `apps/desktop/src/identity.rs`, `apps/desktop/src/main.rs`, identity tests and launch docs.

**Interfaces:** `load_or_create(profile: Option<&str>) -> io::Result<DeviceIdentity>` uses the default path when absent and a validated named subdirectory when present. CLI `--profile <name>` selects a local profile.

- [ ] Write tests proving default path stability, distinct IDs for two profiles and rejection of invalid/path-traversal names.
- [ ] Run the identity tests and observe failure.
- [ ] Implement profile selection and bounded path construction.
- [ ] Run identity tests and `cargo test -p sparkdesk-desktop --offline`.

### Task 2: Authenticate and wire the peer channels

**Files:** `apps/desktop/src/peer.rs`, `apps/desktop/src/signaling.rs`, protocol/auth tests.

**Interfaces:** PeerSession stores its `screen` and `control` channels, polls incoming channels, and accepts local/peer IDs plus request ID and identity signer. A signed fresh challenge binds both IDs and request ID before session capabilities become ready.

- [ ] Add failing tests for device-ID/public-key mismatch, invalid signature and replayed/wrong-request challenge.
- [ ] Implement channel discovery/polling and a mutually authenticated session-ready state.
- [ ] Confirm screen/control stay unavailable until both peers verify.
- [ ] Run the Rust tests and `cargo check --workspace --offline`.

### Task 3: Bounded screen capture and delivery

**Files:** `apps/desktop/src/screen.rs` (new), `apps/desktop/src/peer.rs`, desktop dependencies, capture tests.

**Interfaces:** `ScreenFrame { width, height, jpeg }`; capture starts only for an authenticated accepted session. Dimensions are at most 1920×1080, JPEG payload at most 1 MiB, chunks at most 16 KiB, configured data-channel send buffer is bounded, and a full buffer drops the frame rather than queueing stale frames.

- [ ] Add failing tests for frame size/dimension validation and stale-frame replacement.
- [ ] Add Windows primary-monitor capture and conservative JPEG encoding outside the UI thread.
- [ ] Send frames only after peer authentication and reject oversized inbound frame payloads.
- [ ] Run Rust tests and Windows `cargo check`.

### Task 4: Explicitly gated Windows input

**Files:** `apps/desktop/src/input.rs` (new), `apps/desktop/src/main.rs`, `apps/desktop/src/signaling.rs`, `apps/desktop/src/peer.rs`.

**Interfaces:** `RemoteInput` is a serde-tagged allowlist for normalized pointer move, pointer button and key events. Host-side permission is per-session, initially false; each accepted event is validated and passed to Windows `SendInput` only when the current authenticated session has control enabled.

- [ ] Add failing tests for bounds, event rate, unknown keys and permission lifecycle.
- [ ] Implement allowlisted input dispatch, permission revoke, rate limit and release-all on every shutdown/error path.
- [ ] Run Rust tests and `cargo check --workspace --offline`.

### Task 5: Live view, host consent and bilingual UX

**Files:** `apps/desktop/ui-react/src/ui/App.tsx`, `components/SessionFeedback.tsx`, new viewer/control component and focused styles, `copy.ts`, `types.ts`.

**Interfaces:** Tauri emits authenticated `screen-frame` updates and session role/control state. Viewer renders frames and sends normalized events through typed Tauri commands. Host gets a persistent control permission toggle with revoke and end-session actions.

- [ ] Add or extract testable frame parsing/coordinate normalization tests for invalid and edge coordinates.
- [ ] Implement viewer, host control consent and status affordances using existing Radix/Lucide conventions.
- [ ] Verify keyboard operation, Russian and English copy, responsive layout and production build.

### Task 6: Two-process run and verification

**Files:** `README.md`, `docs/architecture.md`, launch script only if needed.

- [ ] Document one standard launch and one `--profile helper` launch against the configured signaling URL.
- [ ] Run formatting, Rust tests/build and React production build.
- [ ] Launch both local instances, confirm distinct IDs, accept screen viewing, enable/revoke control, and disconnect; record any environment-dependent limit.

**Commit:** `Add consent-gated live screen and remote control`.
