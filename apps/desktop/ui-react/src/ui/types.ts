export type NetworkState = "not_configured" | "connecting" | "online" | "reconnecting";
export type NoticeState = "idle" | "incoming_request" | "error" | "pending" | "connected";

export type AppStatus = {
  deviceId: string;
  networkState: NetworkState;
  noticeState: NoticeState;
  detail: string;
};
