# SparkDesk signaling Worker

This Worker only coordinates device presence and WebRTC setup. It never carries screen frames or remote input. A Durable Object is named by each device ID; the service asks the target object to forward incoming requests, so one global object does not serialize every device through a single queue.

## Local development

```powershell
npm ci
npm run dev
```

WebSocket endpoint: `ws://127.0.0.1:8787/v1/signal?device_id=<24-hex-id>`.

Run the desktop client against this local Worker by setting `$env:SPARKDESK_SIGNALING_URL = "ws://127.0.0.1:8787"` and starting the Tauri frontend with `npm run tauri -- dev` from `apps/desktop/ui-react`. Two clients on the same machine or LAN can exercise authenticated presence, consent and WebRTC setup. A public deployment is needed for devices outside that network.

Before going online, the client receives a random challenge, signs the domain-separated nonce with its local Ed25519 key, and sends its public key and derived ID. Registration is account-free; the Worker holds online sockets only.

## Build and deployment

```powershell
npm run build
npm run deploy
```

The current test endpoint is a temporary Cloudflare deployment. A persistent deployment requires a Cloudflare account and stays subject to the free plan's quotas. The desktop app must not assume that a free quota is an uptime guarantee. This Worker is not a screen relay; direct connectivity can still fail behind some routers. Screen capture and remote input are not implemented yet.
