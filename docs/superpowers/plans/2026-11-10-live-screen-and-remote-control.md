# Remote Control Responsiveness and Release Readiness Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task by task. Do not start the desktop app or change code until the user asks to execute the plan.

**Goal:** Make a consent-based SparkDesk session reliably show a partner's live Windows desktop and let the viewer operate it with their own mouse and keyboard at measured, usable latency.

**Architecture:** Keep Tauri/React for the interface, Rust for Windows capture/input and the security boundary, and the current direct WebRTC peer connection for transport. First reproduce and measure the existing JPEG/data-channel path; improve it only against measurements, with a time-bounded video-transport proof of concept if the current path cannot meet the agreed LAN target. Test internet traversal separately and expose an explicit direct-only limitation if relay service is not configured.

**Tech Stack:** Rust 2024, Tauri 2, current `webrtc` dependency, Windows `xcap`/`SendInput`, React/Vite, existing Radix Themes and Lucide components.

**Spec:** `docs/superpowers/specs/2026-11-10-live-screen-and-remote-control-design.md`

## What the user should be able to do

1. Enter a partner's ID and request a session.
2. The partner sees and accepts the request before any screen is shared.
3. The viewer sees the partner's live Windows desktop.
4. The partner separately enables remote control; the viewer's own mouse and keyboard then operate that computer.
5. The partner can revoke control immediately or end the session. Both people can tell whether viewing and control are active.

## Roadmap

| Stage | Deliverable | Continue only when |
| --- | --- | --- |
| 1. Prove the existing path | Two local clients show a live screen and pass real input; baseline measurements are recorded. | The basic accept, view, click and type flow works. |
| 2. Make control dependable | Pointer, keys, consent, revoke and disconnect behave correctly, including held-input cleanup. | No input leaks across a revoke or disconnect. |
| 3. Make it responsive | Screen delivery is measured and bounded; stale frames are dropped; control stays responsive. | Same-LAN targets below pass, or a separate video-transport decision is ready. |
| 4. Validate connectivity | Direct and, if needed, TURN-relayed connections work across separate networks. | The actual ICE path is known and documented. |
| 5. Prepare a release | UI states, keyboard use, security limits and Windows installer are checked end to end. | Every final acceptance item has evidence from real Windows clients. |

Stages are sequential: do not tune frame rate before proving the basic session, and do not claim internet support from a signaling-only or same-LAN test.

## Scope

- First release: Windows host and viewer, one primary monitor, attended sessions, screen viewing and consent-gated mouse/keyboard control.
- Explicitly out of scope: unattended access, stealth, privilege/UAC bypass, secure desktop control, file transfer, audio and clipboard sync.
- Internet paths that require TURN are supported only after TURN credentials and relay behavior are configured and tested. Never commit relay credentials.
- Multi-monitor selection and non-Windows hosts are separate follow-up work; do not imply they are supported by this plan.

## Baseline

- The code already has separate `screen` and `control` data channels, JPEG capture, Rust input validation, a React viewer, separate local profiles and a host-side control permission.
- Before the 60 FPS work, capture topped out at 1280×720, encoded JPEG quality 45 and requested a frame every 120 ms (about 8 frames/second). Frames are base64-encoded for UI events.
- Rust tests and the UI build have passed, but the actual screen, mouse and keyboard path has not yet been exercised between two Windows computers.
- Treat the first task as a live reproduction and measurement gate. Do not claim the desktop is ready for remote support until the host and viewer have completed the acceptance run below.

## Local implementation status (2026-11-11)

- The sender now requests frames on a 60 Hz schedule, skips missed ticks, pauses new capture when the screen-channel backlog reaches 256 KiB, and creates chunks lazily. The screen channel is unordered with zero retransmits so stale image data is not retransmitted ahead of new frames; control remains on its separate reliable channel.
- The frame assembler rejects late chunks from older completed frames. The viewer draws only the latest decoded frame to a canvas on animation frames, without a React update per image, and reports its canvas draw rate plus a rolling p95 input RTT. Input actions carry a bounded per-session sequence number and the host acknowledges only after OS dispatch; a failed dispatch is reported as not applied.
- A connected session opens in its own native window while the home window remains available. Closing the session window ends the session. Host permission remains off after accept and can be revoked at any time; the existing revoke path releases held input.
- Rust tests cover the session-window eligibility gate, acknowledgement message, stale frames and pointer mapping to all four monitor edges with a negative origin.
- These are code-level changes, not performance evidence. No desktop app was launched and no two-machine session, 60-second FPS run, input RTT measurement, TURN path test, ten-minute soak or installer verification was performed. Keep those gates open until the runs have recorded results.
- The 60 FPS and 50 ms p95 targets remain goals to measure, not claims about current performance. JPEG encoding, CPU, network bandwidth, base64 conversion and React rendering may keep the current path below target; if so, use measured data to decide whether a video transport is needed.

## Execution record (2026-11-11)

- Added regression coverage for pausing capture at the existing screen-channel backlog threshold and for replacing an incomplete frame with a newer frame. The implementation already had both behaviors; tests confirm them, so no transport tuning was justified without the live baseline.
- Added per-session local diagnostics for capture/resize/encode timing, JPEG payload bytes, frame IDs and counts, backlog skips, incomplete frames/stale chunks, input send/dispatch/ack, and WebRTC connection state. The session window shows them in a keyboard-accessible bilingual disclosure; metrics contain no frame pixels, typed text or device IDs.
- Added bounded reporting of viewer-rendered frame counts once per second. Input RTT p50/p95 now includes all acknowledgements, including rejected OS dispatches, so a failed remote action still contributes its actual round-trip time.
- Expanded the timing breakdown to include chunk-send time (including send-buffer waits), completed-frame reassembly latency and the synchronous Tauri event-emission call. The WebRTC crate's configured 512 KiB cap blocks each channel's `send` until capacity returns; the app separately skips capture at 256 KiB outstanding before a frame starts.
- Added held-input release tests; if Windows rejects a key/button-up event, SparkDesk retains that held input for retry on the next permission poll.
- Viewer diagnostics now record the host's `applied` result from every input acknowledgment, while still including rejected actions in RTT samples. The data-plane loop emits a final diagnostics snapshot after counting any incomplete frame at shutdown.
- Added `docs/remote-control-validation.md` with source-configured values and an explicit pending row for real measurements. No hardware, FPS or latency results were fabricated.
- Rechecked README claims: Windows primary-monitor scope, separate host consent, direct ICE with no TURN configuration, and protected Windows surfaces are documented.
- `cargo fmt --all -- --check`, `cargo test --workspace --offline` (35 tests), `cargo build -p sparkdesk-desktop --offline`, and the React production build pass. NSIS packaging succeeded before these latest code changes; it must be rebuilt before install verification. MSI packaging fails with WiX `LGHT0217` because Windows Installer Service is unavailable here. The NSIS installer was not installed.
- The live two-window interaction gate could not be performed in this execution environment: launched backend processes connected to signaling, but no interactive windows were exposed. Therefore the release gates remain open for real screen/input interaction, measured performance, separate-network ICE, soak, and fresh-install evidence. Do not treat this record as a release sign-off.

## Remaining acceptance gates (2026-11-11)

- **Tasks 1–2:** The two-client accept/view/control flow and pointer, keyboard, revoke, disconnect and focus-loss behavior still need an interactive Windows run. Earlier launches connected to signaling but exposed no interactive windows, so no input actions or frame results are claimed.
- **Tasks 3–4:** Instrumentation and code-level pointer mapping checks are present. Measured 1280×720 stage timings, 60-second displayed FPS/RTT/CPU, resized/letterboxed corner mapping, full-screen behavior and keyboard-only navigation still need a live viewer/host run.
- **Task 5:** Source configuration is Cloudflare STUN only, with no TURN settings. Same-LAN and separate-network candidate pairs are not recorded; TURN implementation stays conditional on a measured direct-connect failure.
- **Task 6:** Code-level security checks and builds are recorded. Consent/revoke behavior, a 10-minute session and a fresh installer install/launch remain unverified; the prior NSIS artifact predates the current code.

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
- Sustain 60 displayed frames per second for 60 seconds at 1280×720 on the reference wired LAN. Record effective resolution, receive/render intervals and skipped frame IDs; text on the host desktop must remain legible.
- Remote input acknowledgement round-trip p95 at or below 50 ms on the same LAN. Also report p50 and p95; do not infer one-way latency by assuming clocks on two machines match.
- No input accepted before consent or after revoke, and no held key/button remains after disconnect.
- A separate-network run must complete with the configured ICE path. If it needs TURN, document the provider/configuration and test relay traffic before calling internet connectivity supported.

Targets are a test gate, not a guarantee over arbitrary internet links. A 60 Hz capture timer alone does not count as 60 FPS; the measured viewer render rate must meet the target. Record deviations and their measured cause rather than hiding them by lowering the target.

## Implementation Tasks

### Task 1: Reproduce both roles and capture a baseline

**Files:** `apps/desktop/src/peer.rs`, `apps/desktop/src/screen.rs`, `apps/desktop/src/input.rs`, `apps/desktop/ui-react/src/ui/components/RemoteViewer.tsx`, `README.md`, and this plan's linked spec.

- [ ] Start two windowed desktop instances on one Windows machine using the documented default and `--profile=helper` identities; verify both distinct IDs are online.
- [ ] Run the owner-accept / viewer-connect flow. Record whether frames arrive and whether pointer, buttons, wheel, ordinary keys and modifiers reach the host.
- [x] Add bounded diagnostic counters for capture/resize/encode duration, bytes per frame, frame IDs sent/received/rendered, dropped/incomplete chunks, input send/dispatch/ack and connection state. Keep diagnostics local; never record frame contents, typed text or device secrets.
- [x] Add an input acknowledgement carrying a per-session sequence number after host validation and OS dispatch. The viewer measures round-trip with its monotonic clock; it does not compare unsynchronized host/viewer clocks.
- [ ] Write the baseline to a new `docs/remote-control-validation.md` with the machine/network details and measured p50/p95 values.
  - [x] Added the report and recorded configured limits; real hardware, path and percentile results remain pending.
- [x] Update the spec's `Current state` and transport limits to describe the implemented code and remaining non-goals.
- [x] Run `cargo test --workspace --offline` and `npm --prefix apps/desktop/ui-react run build`.

**Gate:** If the real local two-instance session cannot show a moving screen and perform a basic click and text entry, stop performance tuning and fix that reproduced functional failure first.

### Task 2: Prove input correctness and safe release

**Files:** `apps/desktop/src/input.rs`, `apps/desktop/src/peer.rs`, `apps/desktop/src/main.rs`, `apps/desktop/ui-react/src/ui/components/RemoteViewer.tsx`.

- [x] Add a failing Rust test named `revoking_control_releases_all_held_keys_and_buttons` covering a held modifier and mouse button.
- [x] Add a Rust test named `pointer_coordinates_map_to_capture_monitor_edges` covering all four edges and a non-zero/negative monitor origin.
- [x] Run the held-input cleanup test and confirm it fails for the missing release seam before implementation.
- [ ] Verify `pointercancel`, viewer blur and control revoke in the real two-instance run.
- [x] Implement the smallest fixes in the existing input/viewer flow; preserve allowlists, payload bounds and the current per-second rate limit unless Task 1 measurements prove it rejects normal input.
- [x] Re-run the focused tests, then `cargo test --workspace --offline` and the UI build.
- [ ] On the two-instance Windows run, verify click, drag, right-click, wheel, text entry, Ctrl/Alt/Shift combinations, revoke while held, disconnect while held and Alt+Tab. Confirm no action reaches the host after revoke.

**Gate:** No test or manual step may show an accepted OS input before host permission or a stuck input after revoke/disconnect.

### Task 3: Remove frame lag using measured back-pressure

**Files:** `apps/desktop/src/screen.rs`, `apps/desktop/src/peer.rs`, `apps/desktop/ui-react/src/ui/components/RemoteViewer.tsx`, `apps/desktop/ui-react/src/ui/types.ts`, `docs/remote-control-validation.md`.

- [x] Add regression tests named `frame_send_policy_pauses_capture_when_channel_is_backlogged` and `frame_assembler_discards_superseded_incomplete_frame`; keep the queue policy small and independent of WebRTC so it can be tested without a live peer. Also verify incomplete-frame accounting when the session ends.
- [x] Run the tests and confirm the existing backlog and superseding behavior; the missing incomplete-at-shutdown accounting test failed before its implementation.
- [ ] Measure capture, resize, JPEG encode, chunk send, receive/reassembly, UI event and render intervals separately at 1280×720. Confirm the displayed frame rate against Task 1's baseline.
- [x] Bound in-flight screen data using the WebRTC channel's buffered amount or an equivalent single-frame budget. Skip new capture when backlogged; keep control on its separate channel.
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
- [x] Run the focused Rust mapping test, `cargo test --workspace --offline` and the UI build (code/build checks only; interactive window behavior still needs a real run).

### Task 5: Validate direct and relayed network paths

**Files:** `apps/desktop/src/peer.rs`, `apps/desktop/src/signaling.rs`, `apps/desktop/tauri.conf.json` or existing runtime configuration, `README.md`, `docs/remote-control-validation.md`.

- [ ] Inspect the current ICE server configuration and record the selected candidate pair for each test; do not infer direct versus relayed transport from a successful signaling connection.
  - [x] Confirmed the current code configures Cloudflare STUN only and has no TURN configuration; no candidate pair can be recorded before live network tests.
- [ ] Test two computers on the same LAN, then two computers on separate networks. Record connection setup time, selected ICE path, frame rate and input RTT.
- [ ] If separate-network direct ICE fails, add and run a failing Rust test named `ice_configuration_requires_credentials_for_turn_servers` for valid STUN, valid TURN and missing TURN credentials.
- [ ] Implement configurable STUN/TURN server settings and short-lived TURN credentials without hard-coded secrets; keep signaling free of screen payloads.
- [ ] Test successful TURN fallback, missing/expired credentials and relay disconnection. Show a truthful connection/error state in both Russian and English.
- [ ] Document whether internet connectivity is direct-only or TURN-backed and the exact required environment/configuration.

**Gate:** Do not market or label arbitrary internet remote access as working until a two-network test succeeds with the intended ICE path.

### Task 6: Final security, performance and release check

**Files:** relevant tests under `apps/desktop/src`, `apps/desktop/ui-react`, `README.md`, `docs/remote-control-validation.md`.

- [x] Verify the Rust boundary tests reject forged identity/session proof, malformed/oversized frames, unknown keys/buttons and input floods; frame-flood tests confirm malformed frames retain no state and incomplete-frame floods retain one bounded frame.
- [ ] Verify decline sends no screen frames; control starts off; revoke stops input immediately while viewing may continue; disconnect stops capture and releases all held inputs.
- [ ] Run a 10-minute two-PC session while moving the pointer and typing; record frame rate, RTT, memory and CPU, and confirm the UI remains responsive.
- [x] Run `cargo fmt --all -- --check`, `cargo test --workspace --offline` (35 tests), `cargo build -p sparkdesk-desktop --offline`, and `npm --prefix apps/desktop/ui-react run build` (code verification only; no two-client acceptance implied).
- [ ] Build the existing Windows installer target and verify fresh install, first launch and normal-size startup without reusing the development profile or credentials.
- [x] README documents attended Windows sharing, owner consent, separate control permission, direct-only connectivity without TURN, and Windows protected-surface limitations.

**Definition of done:** All gates above have recorded evidence from real Windows clients. A successful compile, browser preview, or single-process test alone does not complete this plan.
