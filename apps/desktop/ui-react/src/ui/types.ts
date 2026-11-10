export type NetworkState = "not_configured" | "connecting" | "online" | "reconnecting";
export type NoticeState = "idle" | "incoming_request" | "error" | "pending" | "connected";
export type SessionRole = "none" | "host" | "viewer";

export type RemoteInput =
  | { type: "pointer_move"; x: number; y: number }
  | { type: "pointer_button"; button: "left" | "right" | "middle" | "back" | "forward"; down: boolean; x: number; y: number }
  | { type: "pointer_wheel"; delta: number; horizontal: boolean; x: number; y: number }
  | { type: "key"; code: string; down: boolean };

export type ScreenFrame = { id: number; width: number; height: number; jpegBase64: string };

export type AppStatus = {
  deviceId: string;
  networkState: NetworkState;
  noticeState: NoticeState;
  sessionRole: SessionRole;
  controlEnabled: boolean;
  detail: string;
};
