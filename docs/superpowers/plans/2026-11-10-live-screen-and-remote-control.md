# Remote Control Responsiveness and Release Readiness Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task by task. Do not start the desktop app or change code until the user asks to execute the plan.

**Goal:** Make a consent-based SparkDesk session reliably show a partner's live Windows desktop and let the viewer operate it with their own mouse and keyboard at measured, usable latency.

**Architecture:** Keep Tauri/React for the interface, Rust for Windows capture/input and the security boundary, and the current direct WebRTC peer connection for transport. First reproduce and measure the existing JPEG/data-channel path; improve it only against measurements, with a time-bounded video-transport proof of concept if the current path cannot meet the agreed LAN target. Test internet traversal separately and expose an explicit direct-only limitation if relay service is not configured.

**Tech Stack:** Rust 2024, Tauri 2, current `webrtc` dependency, Windows `xcap`/`SendInput`, React/Vite, existing Radix Themes and Lucide components.

**Spec:** `docs/superpowers/specs/2026-11-10-live-screen-and-remote-control-design.md`

## Baseline

- The code already has separate `screen` and `control` data channels, JPEG capture, Rust input validation, a React viewer, separate local profiles and a host-side control permission.
- Capture currently tops out at 1280×720, encodes JPEG quality 45 and requests a frame every 120 ms (about 8 frames/second). Frames are base64-encoded for UI events.
- Rust tests and the UI build have passed, but the actual screen, mouse and keyboard path has not yet been exercised between two Windows computers.
- Treat the first task as a live reproduction and measurement gate. Do not claim the desktop is ready for remote support until the host and viewer have completed the acceptance run below.

## Global Constraints

- A device ID locates a peer; it never grants screen or input access.
- Screen capture begins only after the host accepts the session. Remote input is off until the host separately enables it and can revoke it immediately.
- Keep frame memory and queued transport work bounded; stale screen frames must not build up behind newer frames.
- Keep input on its existing separate channel so image delivery cannot intentionally queue ahead of mouse or keyboard actions.
- Validate all remote data at the Rust boundary. Release held keys and buttons on revoke, blur, disconnect and failure.
- Use only documented Windows input APIs. Do not bypass UIPI, UAC, secure desktops or another process's elevation boundary.
- Keep the current React/Tauri framework, Radix components, Lucide icons and English/Russian support.
- Start in a normal-size window. Full screen remains an explicit user action.
- Never route screen images through the signaling service. Any relay must carry encrypted WebRTC traffic and use credentials from configuration, never source code.

## Review Focus

- Revoking permission while a key or mouse button is held must release it on the host immediately.
- A slow or lossy network must not create an ever-growing queue of stale frames or delay control messages behind image chunks.
- Resizing, letterboxing, multi-monitor offsets and negative monitor origins must still place the remote pointer at the intended pixel.
- Viewer focus loss, Alt+Tab, window close, reconnect and host disconnect must not leave synthetic input pressed.
- Direct connectivity failure or unavailable relay credentials must produce a clear connection state; the UI must not imply that remote control succeeded when no action reached the host.

## Acceptance Targets

Record hardware, Windows version, capture resolution, network path and results with every run. Initial measurable targets for a wired same-LAN run are:

- First usable screen frame within 2 seconds after accepted connection.
- At least 20 displayed frames per second over a 60-second 1280×720 run. Record receive/render intervals and skipped frame IDs to detect stale queued work without comparing unsynchronized machine clocks.
- Remote input acknowledgement round-trip p95 at or below 100 ms on the same LAN. Also report p50 and p95; do not infer one-way latency by assuming clocks on two machines match.
- No input accepted before consent or after revoke, and no held key/button remains after disconnect.
- A separate-network run must complete with the configured ICE path. If it needs TURN, document the provider/configuration and test relay traffic before calling internet connectivity supported.

Targets are a test gate, not a guarantee over arbitrary internet links. Record deviations and their measured cause rather than hiding them by lowering the target.

## Implementation Tasks

### Task 1: Reproduce both roles and capture a baseline

**Files:** `apps/desktop/src/peer.rs`, `apps/desktop/src/screen.rs`, `apps/desktop/src/input.rs`, `apps/desktop/ui-react/src/ui/components/RemoteViewer.tsx`, `README.md`, and this plan's linked spec.

- [ ] Start two windowed desktop instances on one Windows machine using the documented default and `--profile=helper` identities; verify both distinct IDs are online.
- [ ] Run the owner-accept / viewer-connect flow. Record whether frames arrive and whether pointer, buttons, wheel, ordinary keys and modifiers reach the host.
- [ ] Add bounded diagnostic counters for capture/encode duration, bytes per frame, frame IDs sent/received/rendered, dropped/incomplete frames, input send/dispatch/ack and connection state. Keep diagnostics local; never record frame contents, typed text or device secrets.
- [ ] Add an input acknowledgement carrying a per-session sequence number after host validation and OS dispatch. Measure round-trip with the viewer's monotonic clock; do not compare unsynchronized host/viewer clocks.
- [ ] Write the baseline to a new `docs/remote-control-validation.md` with the machine/network details and measured p50/p95 values.
- [ ] Update the spec's `Current state`, transport limits and non-goals so it describes the shipped code instead of the pre-implementation state.
- [ ] Run `cargo test --workspace --offline` and `npm --prefix apps/desktop/ui-react run build`.

**Gate:** If the real local two-instance session cannot show a moving screen and perform a basic click and text entry, stop performance tuning and fix that reproduced functional failure first.

### Task 2: Prove input correctness and safe release

**Files:** `apps/desktop/src/input.rs`, `apps/desktop/src/peer.rs`, `apps/desktop/src/main.rs`, `apps/desktop/ui-react/src/ui/components/RemoteViewer.tsx`.

- [ ] Add a failing Rust test named `revoking_control_releases_all_held_keys_and_buttons` covering a held modifier and mouse button.
- [ ] Add a failing test named `pointer_coordinates_map_to_capture_monitor_edges` covering all four edges and a non-zero/negative monitor origin.
- [ ] Run each new test and confirm it fails for the missing behavior before implementation.
- [ ] Verify `pointercancel`, viewer blur and control revoke in the real two-instance run.
- [ ] Implement the smallest fixes in the existing input/viewer flow; preserve allowlists, payload bounds and the current per-second rate limit unless Task 1 measurements prove it rejects normal input.
- [ ] Re-run the focused tests, then `cargo test --workspace --offline` and the UI build.
- [ ] On the two-instance Windows run, verify click, drag, right-click, wheel, text entry, Ctrl/Alt/Shift combinations, revoke while held, disconnect while held and Alt+Tab. Confirm no action reaches the host after revoke.

**Gate:** No test or manual step may show an accepted OS input before host permission or a stuck input after revoke/disconnect.

### Task 3: Remove frame lag using measured back-pressure

**Files:** `apps/desktop/src/screen.rs`, `apps/desktop/src/peer.rs`, `apps/desktop/ui-react/src/ui/components/RemoteViewer.tsx`, `apps/desktop/ui-react/src/ui/types.ts`, `docs/remote-control-validation.md`.

- [ ] Add failing tests named `frame_send_policy_drops_stale_frames_when_channel_is_backlogged` and `frame_assembler_discards_superseded_incomplete_frame`; keep the queue policy small and independent of WebRTC so it can be tested without a live peer.
- [ ] Run both tests and confirm they fail for stale-frame handling before changing the sender or assembler.
- [ ] Measure capture, resize, JPEG encode, chunk send, receive/reassembly, UI event and render intervals separately at 1280×720. Confirm the displayed frame rate against Task 1's baseline.
- [ ] Bound in-flight screen data using the WebRTC channel's buffered amount or an equivalent single-frame budget. Drop an obsolete frame instead of queueing it; keep the separate control channel responsive.
- [ ] Select a capture interval, JPEG quality and downscale policy from the measurements. Keep the first pass on the existing JPEG transport and document chosen values; do not add a codec dependency before profiling.
- [ ] Avoid repeated full-size base64/string and React work where the current Tauri event/render path shows a measurable cost. Keep frame updates local to the viewer.
- [ ] Re-run sender/reassembly tests and the 60-second LAN benchmark; record displayed FPS, receive/render intervals, skipped IDs, input RTT and CPU before/after.
- [ ] If the current data-channel path still misses the acceptance target after bounded back-pressure and one measured tuning pass, stop. Write a short video-transport spike proposal comparing implementation cost and measured expected benefit; do not silently expand this task into a codec rewrite.

**Gate:** Pass the same-LAN acceptance targets, or return with measurements and a separately reviewable transport-spike proposal. Never claim “no lag” based on a build alone.

### Task 4: Verify pointer mapping and ordinary window behavior

**Files:** `apps/desktop/ui-react/src/ui/components/RemoteViewer.tsx`, `SessionPage.tsx`, `App.tsx`, `apps/desktop/src/input.rs`, `apps/desktop/tauri.conf.json`.

- [ ] Verify clicks at each visible image corner land on the corresponding remote desktop corner with both letterboxed and matching aspect ratios.
- [ ] Verify pointer coordinates after resizing the window and on a host monitor with a non-zero desktop origin.
- [ ] Verify the app launches at its configured normal dimensions and does not maximize or enter full screen when a session connects.
- [ ] Verify the explicit full-screen button and F11 enter/exit full screen, and Escape exits only when full screen is active.
- [ ] Verify keyboard-only operation for connect, consent, control permission, revoke and disconnect. Preserve visible focus and accessible names.
- [ ] Run the focused Rust mapping test, `cargo test --workspace --offline` and the UI build.

### Task 5: Validate direct and relayed network paths

**Files:** `apps/desktop/src/peer.rs`, `apps/desktop/src/signaling.rs`, `apps/desktop/tauri.conf.json` or existing runtime configuration, `README.md`, `docs/remote-control-validation.md`.

- [ ] Inspect the current ICE server configuration and record the selected candidate pair for each test; do not infer direct versus relayed transport from a successful signaling connection.
- [ ] Test two computers on the same LAN, then two computers on separate networks. Record connection setup time, selected ICE path, frame rate and input RTT.
- [ ] If separate-network direct ICE fails, add and run a failing Rust test named `ice_configuration_requires_credentials_for_turn_servers` for valid STUN, valid TURN and missing TURN credentials.
- [ ] Implement configurable STUN/TURN server settings and short-lived TURN credentials without hard-coded secrets; keep signaling free of screen payloads.
- [ ] Test successful TURN fallback, missing/expired credentials and relay disconnection. Show a truthful connection/error state in both Russian and English.
- [ ] Document whether internet connectivity is direct-only or TURN-backed and the exact required environment/configuration.

**Gate:** Do not market or label arbitrary internet remote access as working until a two-network test succeeds with the intended ICE path.

### Task 6: Final security, performance and release check

**Files:** relevant tests under `apps/desktop/src`, `apps/desktop/ui-react`, `README.md`, `docs/remote-control-validation.md`.

- [ ] Verify forged identity, invalid session proof, malformed/oversized frames, unknown key/button, input flood and frame flood cannot grant capability, panic or grow memory without bound.
- [ ] Verify decline sends no screen frames; control starts off; revoke stops input immediately while viewing may continue; disconnect stops capture and releases all held inputs.
- [ ] Run a 10-minute two-PC session while moving the pointer and typing; record frame rate, RTT, memory and CPU, and confirm the UI remains responsive.
- [ ] Run `cargo fmt --all -- --check`, `cargo test --workspace --offline`, `cargo build -p sparkdesk-desktop --offline`, and `npm --prefix apps/desktop/ui-react run build`.
- [ ] Build the existing Windows installer target and verify fresh install, first launch and normal-size startup without reusing the development profile or credentials.
- [ ] Update README with supported Windows behavior and explicit limitations, including UIPI/secure desktop and any direct-only network restriction.

**Definition of done:** All gates above have recorded evidence from real Windows clients. A successful compile, browser preview, or single-process test alone does not complete this plan.
