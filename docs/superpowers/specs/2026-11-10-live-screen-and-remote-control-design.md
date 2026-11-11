# Live screen and remote control design

## Intent

Turn the current approved direct WebRTC connection into a usable Windows remote-support session: the helper sees the other device's live screen and can send mouse and keyboard input only when the device owner explicitly enables control.

## Current state

The desktop uses Rust/Tauri for identity, signaling, WebRTC, Windows capture and input dispatch, with a React UI. An accepted connection opens a dedicated session window while the home window remains available. Screen frames use bounded JPEG chunks over an unordered, zero-retransmit data channel; control uses a separate reliable channel. The viewer reports rendered frame rate and input-dispatch RTT, and the session window exposes local bounded timing/counter diagnostics for capture, encoding, send, reassembly and event emission. A two-computer performance and connectivity run has not yet been recorded.

## Goals

- Capture the Windows desktop only after an incoming session is accepted.
- Display the live image in the connecting app and send pointer/keyboard events over the direct encrypted WebRTC connection.
- Keep remote control off until the local user explicitly enables it; show a persistent local indicator and allow immediate revocation/end.
- Validate message type, coordinates, key/button codes, message size and event rate on the receiving Rust boundary.
- Bind each WebRTC session to the expected device IDs with a signed challenge over the data channel before sending screen frames or accepting control input.
- Stop capture and release any held synthetic inputs on disconnect or revocation.
- Keep the session window independent from the home window; closing the session window ends the active session.
- Report canvas draw rate and p95 input acknowledgement RTT from the viewer's monotonic clock.
- Allow a second local test instance to use a separate identity/profile without changing the normal persistent identity.
- Preserve React/Tauri UI and existing English/Russian support.

## Non-goals

- Unattended access, stealth capture, privilege elevation, UAC bypass, file transfer, audio, clipboard synchronization, mobile clients, TURN deployment, installer release, or claims of support for protected/secure desktops.

## Approaches

1. **Native Windows capture plus the existing WebRTC transport (recommended):** capture frames in Rust, encode bounded JPEG frames, send them over the pre-created `screen` data channel, render them in React, and validate/dispatch user input over `control`. This reuses the current authenticated signaling and DTLS-encrypted peer transport and keeps OS access in Rust. Initial frame rate/resolution will be conservative and measured; this is a functional first remote-support slice, not a codec-optimized production streamer.
2. **Encoded WebRTC video track:** use a Windows capture source and a real video encoder/track. This offers better congestion handling and video efficiency, but adds encoder/decoder integration and substantially more platform and browser testing.
3. **Screenshot polling over signaling or a hosted relay:** rejected because it would route private screen content through the signaling service and add latency/cost.

## Security and behavior

- An ID locates a device but never grants access. Every incoming session still requires local acceptance.
- Screen viewing begins only after acceptance. Keyboard and mouse control requires a second explicit local action and starts disabled.
- The local host can revoke control immediately without ending viewing; held keys/buttons are released on the host. Ending the session closes the media path and releases any remaining held inputs.
- Data-channel input is untrusted even after consent. Rust checks an allowlist of input event variants and bounds/rates before calling Windows APIs. Unsupported keys, out-of-range coordinates and oversized messages are ignored.
- Windows synthetic input uses documented `SendInput` behavior and cannot bypass User Interface Privilege Isolation (UIPI), secure desktops or elevation boundaries.
- Capture and image delivery run outside the UI thread. Capture is scheduled at 60 Hz; missed ticks are skipped, and a backlog threshold pauses the next capture. This is a requested capture rate, not proof of 60 displayed FPS.
- Frames are JPEG, capped at 1 MiB and 1280×720, split into 16,000-byte payload chunks. The screen data channel is unordered and does not retransmit lost chunks; the assembler rejects late chunks from older frames. Control uses normalized primary-monitor coordinates, an allowlist of keys/buttons and bounded held-input state.
- Accepted input carries a per-session sequence number. The host acknowledges whether Windows dispatched it, and the viewer measures round-trip time with its own monotonic clock.
- A peer's identity is verified from its public key (the device ID is derived from that key) and a signature over a fresh nonce, request ID and both device IDs before either side grants session capabilities.
- The local test profile is a validated profile name under SparkDesk's configuration directory, never an arbitrary key-file path; the default profile remains untouched.

## First-run/test path

Both processes use the same configured signaling URL. The second local process gets a separate explicit identity profile so each device has a distinct ID; normal launches continue to use the default identity. A two-device test remains necessary for real NAT/firewall coverage.

## Acceptance criteria

- Two independent IDs can be online simultaneously, including two local instances using separate named profiles; an invalid profile name cannot escape the SparkDesk configuration directory.
- A peer that fails the signed session challenge receives no frames and cannot send accepted input.
- After host acceptance, the helper displays updated screen frames; declining transmits no frames.
- Control events have no effect before the host enables control, and stop immediately after revocation or disconnect.
- Invalid or excessive control messages cannot trigger OS input.
- UI remains responsive, keeps home and session in separate windows, provides a host control/revoke toggle, and reports viewer frame rate and input p95.
- Rust tests cover protocol validation and control gating; React builds; a real two-instance Windows test verifies capture, display, mouse, keyboard, revoke and disconnect.

## Scope call

The initial implementation will use the existing Rust/WebRTC core and React viewer rather than moving the product UI back to Slint. The screen path will use bounded compressed frames over the existing `screen` channel as the smallest path to a working end-to-end session; if measurements show it cannot meet acceptable responsiveness, the next step is a video track/codec, not routing frames through the signaling service.
