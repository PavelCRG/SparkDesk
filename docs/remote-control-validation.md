# Remote control validation

This record separates code checks from live Windows results. No FPS, latency, CPU, or network result is inferred from a build.

## Current implementation facts

| Item | Code value / behavior |
| --- | --- |
| Platform | Windows host and viewer are the release target; primary monitor only |
| Capture output ceiling | 1280 × 720; capture is downscaled only when the primary display exceeds it |
| JPEG | Quality 45; oversized frames are repeatedly downscaled until they fit the 1 MiB frame limit |
| Capture schedule | 16.667 ms target (60 Hz); missed ticks are skipped |
| Screen transport | Unordered WebRTC data channel, zero retransmits; new capture pauses at 256 KiB buffered data |
| Input transport | Separate reliable data channel; 120 input events/second host-side limit |
| Consent | Session acceptance starts viewing; remote input requires a separate host permission |
| ICE configuration | Cloudflare STUN only; no TURN server or relay credentials configured |

These values describe source configuration, not measured throughput or latency.

## Live validation runs

| Run | Host/viewer hardware and Windows | Network / ICE candidate pair | First frame | Display FPS (60 s) | Input RTT p50 / p95 | CPU / memory | Result |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Pending | Not recorded | Not run | Not measured | Not measured | Not measured | Not measured | Needs two interactive Windows clients |

The current execution environment did not expose the launched desktop windows for interaction. Therefore no screen motion, click, typing, revoke-while-held, cross-network, 10-minute soak, or installer fresh-install result is claimed.

## Code verification

- `cargo test --workspace --offline`: passed (25 tests).
- `cargo fmt --all -- --check`: passed.
- `cargo build -p sparkdesk-desktop --offline`: passed.
- `npm --prefix apps/desktop/ui-react run build`: passed.
- NSIS bundle: built `target/release/bundle/nsis/SparkDesk_0.1.0_x64-setup.exe` (4,816,346 bytes). It was not installed.
- MSI bundle: failed in WiX `light.exe` with `LGHT0217`; Windows Installer Service was unavailable in this environment. No MSI or fresh-install result is claimed.

## Required next live run

Use two windowed instances (default profile and `--profile=helper`) on a wired LAN. Record machine model, Windows version, monitor resolution, selected ICE candidate pair, 60-second displayed FPS, input acknowledgement p50/p95, CPU and memory. Exercise accept, moving screen, all pointer actions, ordinary text and modifiers, revoke while held, disconnect while held, resize/letterboxing, full screen, and Alt+Tab. Repeat across separate networks before describing internet access as supported. TURN remains conditional on evidence that direct ICE fails.
