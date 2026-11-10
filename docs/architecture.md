# SparkDesk architecture

## Goal

Provide account-free remote support: enter a device ID, let the owner approve the session, then view and control that screen. A device ID locates an online device; it is not a password or permission to control it.

## Stack

- Rust 2024, minimum Rust 1.92.
- Slint 1.18 for the native desktop UI; Rust owns application logic.
- Lucide SVG icons embedded in the Slint client; no JavaScript frontend runtime.
- Ed25519 device identity stored in the OS user configuration directory.
- A Rust workspace crate for the shared signaling protocol.
- Cloudflare Worker + Durable Objects for the initial hosted signaling experiment.
- Tokio and a Rust WebRTC implementation are candidates for the desktop transport; prove the media path before committing to capture and codec choices.
- Windows capture and input adapters first, behind platform boundaries.

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
- Review Slint's distribution terms and attribution requirements before publishing an installer.

## Repository shape

```text
apps/desktop/
  src/main.rs       App startup and UI action wiring
  src/identity.rs   Local Ed25519 device identity
  src/signaling.rs  Authenticated signaling connection and reconnect loop
  ui/main.slint     Bilingual home screen, layout and keyboard shortcuts
  ui/brand/         App logo SVGs
  ui/icons/         Bundled Lucide SVG icons and license notice
crates/protocol/    Shared versioned message types and validation
cloud/signaling/    Worker and per-device Durable Object
docs/               Architecture and implementation plan
```

The desktop can connect to a configured signaling endpoint, authenticate its key and show online status. The partner button does not send a join request yet. Consent controls, screen capture, WebRTC media, remote input, deployment and real two-PC sessions remain future work.
