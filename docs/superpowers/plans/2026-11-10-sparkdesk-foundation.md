# SparkDesk implementation plan

**Goal:** Account-free Rust desktop remote support with explicit consent, direct low-latency streaming, and honest connection status.

**Stack:** Rust 2024, Slint 1.18, Ed25519 identity, shared Rust protocol, Cloudflare Worker + Durable Objects for setup signaling, then a Rust WebRTC and Windows capture/input spike. The signaling service must never carry screen video.

## Ten milestones

1. **Desktop foundation — implemented.** Rust workspace, Slint home screen, persistent ID, copy action and partner ID field. UI build checked; no visual screenshot review yet.
2. **Secure identity — implemented.** Local Ed25519 key; device ID comes from the public key; server challenge signature. Key-corruption path fails closed. Windows file permissions still rely on the user's profile ACL.
3. **Shared protocol — implemented.** Versioned Rust message types, ID and payload-size checks. Workspace compiles; protocol serialization and malformed-input cases have not been exercised.
4. **Hosted signaling — local implementation built.** Worker routes challenge-authenticated devices and short-lived consent/connection setup through per-device Durable Objects. Wrangler dry-run succeeds. It is not deployed or tested between clients.
5. **Rust signaling client and online state — implemented groundwork.** Configurable WebSocket connection, challenge authentication, online/offline UI state, incoming-request notice and reconnect backoff. The client has not been exercised against a running Worker yet.
6. **Consent flow.** Show caller ID, accept/reject, handle timeout/cancel/shutdown safely.
7. **Direct peer connection.** Exchange offer/answer and ICE; report direct connection failure. No TURN is included.
8. **Screen viewing.** Windows capture, encode/send and decode/display, with an always-visible stop control and measured latency.
9. **Remote input.** Approved mouse/keyboard data, coordinate mapping, release held keys and immediate stop.
10. **Reliability and packaging.** Diagnostics, Windows installer, licensing/attribution, measured performance and real two-PC verification. Consider TURN only with an approved budget or maintainable free option.

## Acceptance checkpoints

- After 1–3: workspace builds; identity and message contract are stable.
- After 4–6: two clients find each other and safely approve a session.
- After 7–9: two PCs connect, view, control and stop access.
- After 10: clean install, recovery behavior and network limits are documented.

Do not claim a remote session works until it has been exercised between two real PCs. Do not promise a frame rate or universal connectivity without measurements and a relay fallback.
