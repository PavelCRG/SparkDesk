# SparkDesk implementation plan

**Goal:** Account-free Rust desktop remote support with explicit consent, direct low-latency streaming, and honest connection status.

**Stack:** Rust 2024/Tauri 2, React/Vite with Radix Themes and Lucide React, Ed25519 identity, shared Rust protocol, Cloudflare Worker + Durable Objects for setup signaling, and Rust WebRTC. The signaling service must never carry screen video.

## Ten milestones

1. **Desktop foundation — implemented.** Rust/Tauri workspace, React home screen, persistent ID, copy action and partner ID field. Frontend production build and Rust check pass.
2. **Secure identity — implemented.** Local Ed25519 key; device ID comes from the public key; server challenge signature. Key-corruption path fails closed. Windows file permissions still rely on the user's profile ACL.
3. **Shared protocol — implemented.** Versioned Rust message types, ID and payload-size checks. Workspace compiles; protocol serialization and malformed-input cases have not been exercised.
4. **Hosted signaling — implemented.** Worker routes challenge-authenticated devices and short-lived consent/connection setup through per-device Durable Objects. Wrangler dry-run and temporary deployment health check succeed; a two-client session is still unverified.
5. **Rust signaling client and online state — implemented.** Configurable WebSocket connection, challenge authentication, online/offline UI state, incoming-request notice and reconnect backoff. Temporary Worker health is checked; two-client session has not been exercised.
6. **Consent flow — implemented groundwork.** Show caller ID, accept/reject, negotiate the direct connection, and end an active session. Timeout/cancel/shutdown recovery needs more verification.
7. **Direct peer connection — implemented groundwork.** Exchange offer/answer and gather STUN ICE candidates. No TURN is included and two-PC behavior is unverified.
8. **Screen viewing.** Windows capture, encode/send and decode/display, with an always-visible stop control and measured latency.
9. **Remote input.** Approved mouse/keyboard data, coordinate mapping, release held keys and immediate stop.
10. **Reliability and packaging.** Diagnostics, Windows installer, licensing/attribution, measured performance and real two-PC verification. Consider TURN only with an approved budget or maintainable free option.

## Acceptance checkpoints

- After 1–3: workspace builds; identity and message contract are stable.
- After 4–6: two clients find each other and safely approve a session.
- After 7–9: two PCs connect, view, control and stop access.
- After 10: clean install, recovery behavior and network limits are documented.

Do not claim a remote session works until it has been exercised between two real PCs. Do not promise a frame rate or universal connectivity without measurements and a relay fallback.
