import { DurableObject } from "cloudflare:workers";

interface Env {
  DEVICE_HUB: DurableObjectNamespace<DeviceHub>;
}

interface Attachment {
  claimedId: string;
  authenticated: boolean;
  nonce: string;
  issuedAt: number;
}

interface PendingJoin {
  peerId: string;
  expiresAt: number;
}

type Message = Record<string, unknown>;

const VERSION = 1;
const MAX_MESSAGE_BYTES = 24 * 1024;
const MAX_SDP_BYTES = 16 * 1024;
const MAX_ICE_BYTES = 4 * 1024;
const REGISTRATION_DOMAIN = new TextEncoder().encode("SparkDesk signaling registration v1\0");

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === "/health" && request.method === "GET") {
      return Response.json({ status: "ok", service: "sparkdesk-signaling" });
    }
    if (url.pathname !== "/v1/signal") return new Response("Not found", { status: 404 });
    if (request.method !== "GET") return new Response("Method not allowed", { status: 405 });
    if (request.headers.get("Upgrade")?.toLowerCase() !== "websocket") {
      return new Response("WebSocket upgrade required", { status: 426 });
    }

    const deviceId = url.searchParams.get("device_id")?.toUpperCase() ?? "";
    if (!isDeviceId(deviceId)) return new Response("Invalid device ID", { status: 400 });

    const id = env.DEVICE_HUB.idFromName(deviceId);
    return env.DEVICE_HUB.get(id).fetch(request);
  },
};

export class DeviceHub extends DurableObject<Env> {
  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === "/_forward" && request.method === "POST") {
      return this.forwardInternally(request);
    }
    if (url.pathname !== "/v1/signal" || request.headers.get("Upgrade")?.toLowerCase() !== "websocket") {
      return new Response("Not found", { status: 404 });
    }

    const claimedId = url.searchParams.get("device_id")?.toUpperCase() ?? "";
    if (!isDeviceId(claimedId)) return new Response("Invalid device ID", { status: 400 });

    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair);
    const nonce = encodeHex(crypto.getRandomValues(new Uint8Array(32)));
    const attachment: Attachment = {
      claimedId,
      authenticated: false,
      nonce,
      issuedAt: Date.now(),
    };
    server.serializeAttachment(attachment);
    this.ctx.acceptWebSocket(server);
    this.ctx.storage.setAlarm(Date.now() + 60_000);
    send(server, { type: "challenge", nonce });
    return new Response(null, { status: 101, webSocket: client });
  }

  async webSocketMessage(socket: WebSocket, raw: string | ArrayBuffer): Promise<void> {
    if (typeof raw !== "string" || new TextEncoder().encode(raw).byteLength > MAX_MESSAGE_BYTES) {
      socket.close(1009, "invalid message size");
      return;
    }

    let envelope: unknown;
    try {
      envelope = JSON.parse(raw);
    } catch {
      sendError(socket, "invalid_message");
      return;
    }

    if (!isEnvelope(envelope)) {
      sendError(socket, "unsupported_version");
      return;
    }

    const attachment = socket.deserializeAttachment() as Attachment | null;
    if (!attachment?.authenticated) {
      await this.authenticate(socket, attachment, envelope.message);
      return;
    }

    await this.routeMessage(socket, attachment, envelope.message);
  }

  async webSocketClose(socket: WebSocket): Promise<void> {
    socket.close();
  }

  async alarm(): Promise<void> {
    const now = Date.now();
    let nextExpiry: number | undefined;
    for (const socket of this.ctx.getWebSockets()) {
      const attachment = socket.deserializeAttachment() as Attachment | null;
      if (attachment && !attachment.authenticated) {
        const expiry = attachment.issuedAt + 60_000;
        if (now >= expiry) socket.close(1008, "authentication timed out");
        else nextExpiry = Math.min(nextExpiry ?? expiry, expiry);
      }
    }
    for (const prefix of ["incoming:", "outgoing:"]) {
      const joins = await this.ctx.storage.list<PendingJoin>({ prefix });
      for (const [key, pending] of joins) {
        if (now >= pending.expiresAt) await this.ctx.storage.delete(key);
        else nextExpiry = Math.min(nextExpiry ?? pending.expiresAt, pending.expiresAt);
      }
    }
    if (nextExpiry !== undefined) await this.ctx.storage.setAlarm(nextExpiry);
  }

  private async authenticate(socket: WebSocket, attachment: Attachment | null, message: Message): Promise<void> {
    if (!attachment || Date.now() - attachment.issuedAt > 60_000 || message.type !== "authenticate") {
      socket.close(1008, "authentication required");
      return;
    }

    const deviceId = stringField(message, "device_id");
    const publicKeyHex = stringField(message, "public_key");
    const signatureHex = stringField(message, "signature");
    if (!deviceId || !publicKeyHex || !signatureHex || !isDeviceId(deviceId) || !isHex(publicKeyHex, 64) || !isHex(signatureHex, 128)) {
      sendError(socket, "invalid_identity");
      socket.close(1008, "invalid identity");
      return;
    }

    const publicKeyBytes = decodeHex(publicKeyHex);
    const signature = decodeHex(signatureHex);
    if (
      deviceId !== attachment.claimedId ||
      encodeHex(publicKeyBytes.subarray(0, 12)) !== deviceId ||
      !(await verifyRegistrationSignature(publicKeyBytes, signature, decodeHex(attachment.nonce)))
    ) {
      sendError(socket, "invalid_identity");
      socket.close(1008, "identity verification failed");
      return;
    }

    socket.serializeAttachment({ ...attachment, authenticated: true });
    send(socket, { type: "authenticated", device_id: deviceId });
  }

  private async routeMessage(socket: WebSocket, attachment: Attachment, message: Message): Promise<void> {
    const rawRequestId = stringField(message, "request_id");
    const rawTargetId = stringField(message, "target_id");
    if (!isRequestId(rawRequestId) || !rawTargetId || !isDeviceId(rawTargetId)) {
      sendError(socket, "invalid_message");
      return;
    }
    const requestId = rawRequestId.toUpperCase();
    const targetId = rawTargetId.toUpperCase();
    if (targetId === attachment.claimedId) {
      sendError(socket, "invalid_message");
      return;
    }

    switch (message.type) {
      case "join_request": {
        const key = `outgoing:${requestId}`;
        if (await this.ctx.storage.get(key)) {
          sendError(socket, "invalid_message");
          return;
        }
        const expiresAt = Date.now() + 60_000;
        await this.ctx.storage.put(key, { peerId: targetId, expiresAt } satisfies PendingJoin);
        const currentAlarm = await this.ctx.storage.getAlarm();
        if (currentAlarm === null || expiresAt < currentAlarm) await this.ctx.storage.setAlarm(expiresAt);
        const online = await this.forward(targetId, {
          type: "incoming_join",
          request_id: requestId,
          from_id: attachment.claimedId,
        });
        if (!online) await this.ctx.storage.delete(key);
        send(socket, { type: "join_status", request_id: requestId, target_id: targetId, online });
        return;
      }
      case "join_decision": {
        if (typeof message.accepted !== "boolean") break;
        const key = `incoming:${requestId}`;
        const pending = await this.ctx.storage.get<{ peerId: string }>(key);
        if (!pending || pending.peerId !== targetId || Date.now() >= pending.expiresAt) {
          sendError(socket, "request_not_found");
          return;
        }
        await this.ctx.storage.delete(key);
        const delivered = await this.forward(targetId, {
          type: "join_decision",
          request_id: requestId,
          from_id: attachment.claimedId,
          accepted: message.accepted,
        });
        if (delivered && message.accepted) {
          await this.ctx.storage.put(`session:${requestId}`, { peerId: targetId });
        }
        return;
      }
      case "signal": {
        const payload = validateSignal(message.payload);
        if (!payload) break;
        const session = await this.ctx.storage.get<{ peerId: string }>(`session:${requestId}`);
        if (!session || session.peerId !== targetId) {
          sendError(socket, "session_rejected");
          return;
        }
        await this.forward(targetId, {
          type: "signal",
          request_id: requestId,
          from_id: attachment.claimedId,
          payload,
        });
        return;
      }
      case "end_session": {
        const key = `session:${requestId}`;
        const session = await this.ctx.storage.get<{ peerId: string }>(key);
        if (!session || session.peerId !== targetId) {
          sendError(socket, "session_rejected");
          return;
        }
        await this.ctx.storage.delete(key);
        await this.forward(targetId, {
          type: "session_ended",
          request_id: requestId,
          from_id: attachment.claimedId,
        });
        return;
      }
      default:
        break;
    }
    sendError(socket, "invalid_message");
  }

  private async forward(targetId: string, message: Message): Promise<boolean> {
    const id = this.env.DEVICE_HUB.idFromName(targetId);
    const response = await this.env.DEVICE_HUB.get(id).fetch("https://internal/_forward", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(message),
    });
    return response.ok && (await response.json<{ delivered: boolean }>()).delivered;
  }

  private async forwardInternally(request: Request): Promise<Response> {
    const raw = await request.text();
    if (new TextEncoder().encode(raw).byteLength > MAX_MESSAGE_BYTES) {
      return Response.json({ delivered: false }, { status: 413 });
    }
    let message: unknown;
    try {
      message = JSON.parse(raw);
    } catch {
      return Response.json({ delivered: false }, { status: 400 });
    }
    if (!isRecord(message) || typeof message.type !== "string") {
      return Response.json({ delivered: false }, { status: 400 });
    }

    const requestId = stringField(message, "request_id");
    const fromId = stringField(message, "from_id");
    if (!isRequestId(requestId) || !fromId || !isDeviceId(fromId)) {
      return Response.json({ delivered: false }, { status: 400 });
    }
    const canonicalRequestId = requestId.toUpperCase();
    const canonicalFromId = fromId.toUpperCase();
    if (message.type === "incoming_join") {
      const key = `incoming:${canonicalRequestId}`;
      const existing = await this.ctx.storage.get(key);
      if (existing) return Response.json({ delivered: false });
      let delivered = false;
      for (const socket of this.ctx.getWebSockets()) {
        const attachment = socket.deserializeAttachment() as Attachment | null;
        if (attachment?.authenticated && socket.readyState === WebSocket.OPEN) {
          send(socket, message);
          delivered = true;
        }
      }
      if (delivered) {
        const expiresAt = Date.now() + 60_000;
        await this.ctx.storage.put(key, { peerId: canonicalFromId, expiresAt } satisfies PendingJoin);
        const currentAlarm = await this.ctx.storage.getAlarm();
        if (currentAlarm === null || expiresAt < currentAlarm) await this.ctx.storage.setAlarm(expiresAt);
      }
      return Response.json({ delivered });
    }

    if (message.type === "join_decision") {
      const key = `outgoing:${canonicalRequestId}`;
      const pending = await this.ctx.storage.get<PendingJoin>(key);
      if (!pending || pending.peerId !== canonicalFromId || Date.now() >= pending.expiresAt || typeof message.accepted !== "boolean") {
        return Response.json({ delivered: false });
      }
      await this.ctx.storage.delete(key);
      if (message.accepted) {
        await this.ctx.storage.put(`session:${canonicalRequestId}`, { peerId: canonicalFromId });
      }
    } else if (message.type === "signal" || message.type === "session_ended") {
      const key = `session:${canonicalRequestId}`;
      const session = await this.ctx.storage.get<{ peerId: string }>(key);
      if (!session || session.peerId !== canonicalFromId) return Response.json({ delivered: false });
      if (message.type === "session_ended") await this.ctx.storage.delete(key);
    }

    let delivered = false;
    for (const socket of this.ctx.getWebSockets()) {
      const attachment = socket.deserializeAttachment() as Attachment | null;
      if (attachment?.authenticated && socket.readyState === WebSocket.OPEN) {
        send(socket, message);
        delivered = true;
      }
    }
    return Response.json({ delivered });
  }
}

async function verifyRegistrationSignature(
  publicKey: Uint8Array,
  signature: Uint8Array,
  nonce: Uint8Array,
): Promise<boolean> {
  try {
    const key = await crypto.subtle.importKey("raw", publicKey, { name: "Ed25519" }, false, ["verify"]);
    const message = new Uint8Array(REGISTRATION_DOMAIN.length + nonce.length);
    message.set(REGISTRATION_DOMAIN);
    message.set(nonce, REGISTRATION_DOMAIN.length);
    return crypto.subtle.verify({ name: "Ed25519" }, key, signature, message);
  } catch {
    return false;
  }
}

function validateSignal(value: unknown): Message | undefined {
  if (!isRecord(value)) return;
  if (value.kind === "offer" || value.kind === "answer") {
    return typeof value.sdp === "string" && value.sdp.length <= MAX_SDP_BYTES ? value : undefined;
  }
  if (value.kind === "ice_candidate") {
    return typeof value.candidate === "string" && value.candidate.length <= MAX_ICE_BYTES ? value : undefined;
  }
}

function isEnvelope(value: unknown): value is { version: number; message: Message } {
  return isRecord(value) && value.version === VERSION && isRecord(value.message);
}

function isRecord(value: unknown): value is Message {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringField(value: Message, key: string): string | undefined {
  return typeof value[key] === "string" ? value[key] : undefined;
}

function isDeviceId(value: string): boolean {
  return isHex(value, 24);
}

function isRequestId(value: string | undefined): value is string {
  return value !== undefined && isHex(value, 32);
}

function isHex(value: string, length: number): boolean {
  return value.length === length && /^[0-9a-f]+$/i.test(value);
}

function decodeHex(value: string): Uint8Array {
  const bytes = new Uint8Array(value.length / 2);
  for (let i = 0; i < bytes.length; i++) bytes[i] = Number.parseInt(value.slice(i * 2, i * 2 + 2), 16);
  return bytes;
}

function encodeHex(bytes: Uint8Array): string {
  return Array.from(bytes, byte => byte.toString(16).padStart(2, "0")).join("").toUpperCase();
}

function send(socket: WebSocket, message: Message): void {
  if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ version: VERSION, message }));
}

function sendError(socket: WebSocket, code: string): void {
  send(socket, { type: "error", code });
}
