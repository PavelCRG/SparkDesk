# SparkDesk architecture

## Goal

Provide account-free remote support: enter a device ID, let the owner approve the session, then view and control that screen. A device ID locates an online device; it is not a password or permission to control it.

## Stack

- Rust 2024, minimum Rust 1.92.
- Tauri 2 with a React/Vite frontend and Rust application core.
- Radix Themes supplies accessible React controls; Lucide React supplies tree-shaken icon components.
- Ed25519 device identity stored in the OS user configuration directory.
- A Rust workspace crate for the shared signaling protocol.
- Cloudflare Worker + Durable Objects for the initial hosted signaling experiment.
- Tokio and Rust WebRTC provide direct peer transport through separate screen and control data channels.
- Windows capture and input use `xcap` and documented `SendInput` behavior. The Rust boundary validates control messages before dispatch.

## Connection flow

1. The desktop creates a local Ed25519 key on first launch. The displayed ID is derived from its public key; the private key remains local.
2. When configured with a signaling endpoint, the client proves possession of its key with a server challenge. The signaling service keeps online sockets and short-lived session setup state, not permanent accounts.
3. The caller sends a join request to the target ID.
4. The target user explicitly accepts or rejects. An ID alone never authorizes access.
5. After acceptance, clients exchange WebRTC offer/answer and ICE candidates through signaling, then try a direct connection.
6. Screen and input data travel over the encrypted peer connection. Either side can end the session.

The first internet version has no TURN relay. Direct connection is lower-cost and often lower-latency, but some NAT and firewall combinations will fail. Being in the same country does not guarantee connectivity.

## Boundaries

- The signaling service handles presence and connection setup only; it must not carry screen frames or input events.
- Require a visible local approval for every incoming connection. Unattended access is out of scope.
- Validate messages and cap payload sizes at the network boundary.
- Keep capture, encoding, network and input work off the UI event loop.
- Keep only the newest pending video frame so congestion does not build a long delay queue.
- Measure capture-to-display latency, encode time, dropped frames and bitrate before setting quality claims.
- Do not embed long-lived TURN credentials or service secrets in the desktop binary.
- Review Tauri, Radix Themes and Lucide licensing and attribution requirements before publishing an installer.

## Repository shape

```text
apps/desktop/
  src/main.rs       Tauri commands and app state
  src/identity.rs   Local Ed25519 device identity
  src/signaling.rs  Authenticated signaling connection and reconnect loop
  ui-react/src/ui/  Bilingual React interface and session controls
  tauri.conf.json   Window, frontend and capability configuration
crates/protocol/    Shared versioned message types and validation
cloud/signaling/    Worker and per-device Durable Object
docs/               Architecture and implementation plan
```

## Current implementation and limits

The desktop captures the primary Windows monitor after approval, scales frames to at most 1280×720, JPEG-encodes them and sends them over an unordered, zero-retransmit WebRTC data channel. Capture is scheduled at up to 60 frames per second with a bounded send backlog; this is a requested rate, not a measured viewer frame rate.

The viewer displays the newest decoded frame in a canvas. Mouse and keyboard events travel on a separate reliable channel, and the host applies them through `SendInput` only after the host enables control. The host can revoke permission, and held inputs are released on revoke or session end. Windows still blocks synthetic input into elevated applications and secure desktops.

The signaling service uses a configured WebSocket endpoint and coordinates presence and session setup. The peer connection currently uses STUN without TURN, so some NAT and firewall combinations cannot connect. Two-computer performance, internet reachability, installer behavior and sustained-session resource use still need measured Windows runs before SparkDesk can claim release readiness.
