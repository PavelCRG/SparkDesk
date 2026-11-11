export type NetworkState = "not_configured" | "connecting" | "online" | "reconnecting";
export type NoticeState = "idle" | "incoming_request" | "error" | "pending" | "connected";
export type SessionRole = "none" | "host" | "viewer";

export type RemoteInput =
  | { type: "pointer_move"; x: number; y: number }
  | { type: "pointer_button"; button: "left" | "right" | "middle" | "back" | "forward"; down: boolean; x: number; y: number }
  | { type: "pointer_wheel"; delta: number; horizontal: boolean; x: number; y: number }
  | { type: "key"; code: string; down: boolean };

export type ScreenFrame = { id: number; width: number; height: number; jpegBase64: string };
export type InputAck = { sequence: number; applied: boolean };

export type SessionDiagnostics = {
  framesCaptured: number;
  bytesCaptured: number;
  lastCapturedFrameId: number | null;
  framesSent: number;
  bytesSent: number;
  lastSentFrameId: number | null;
  framesReceived: number;
  bytesReceived: number;
  lastReceivedFrameId: number | null;
  framesRendered: number;
  lastRenderedFrameId: number | null;
  backlogSkips: number;
  incompleteFramesDropped: number;
  staleChunksDropped: number;
  inputsSent: number;
  inputsDispatched: number;
  inputsRejected: number;
  inputAcksSent: number;
  inputAcksReceived: number;
  captureSamplesRetained: number;
  captureP50Us: number | null;
  captureP95Us: number | null;
  resizeP50Us: number | null;
  resizeP95Us: number | null;
  encodeP50Us: number | null;
  encodeP95Us: number | null;
  sendP50Us: number | null;
  sendP95Us: number | null;
  reassemblyP50Us: number | null;
  reassemblyP95Us: number | null;
  eventP50Us: number | null;
  eventP95Us: number | null;
  peerState: "connecting" | "connected" | "disconnected" | "failed" | "closed";
};

export type AppStatus = {
  deviceId: string;
  networkState: NetworkState;
  noticeState: NoticeState;
  sessionRole: SessionRole;
  controlEnabled: boolean;
  detail: string;
};
