import { Activity } from "lucide-react";
import type { SessionDiagnostics } from "../types";
import type { Translation } from "../copy";

type SessionDiagnosticsPanelProps = {
  diagnostics: SessionDiagnostics | null;
  t: Translation;
};

function bytes(value: number) {
  if (value >= 1024 * 1024) return `${(value / (1024 * 1024)).toFixed(1)} MiB`;
  if (value >= 1024) return `${(value / 1024).toFixed(1)} KiB`;
  return `${value} B`;
}

export function SessionDiagnosticsPanel({ diagnostics, t }: SessionDiagnosticsPanelProps) {
  if (!diagnostics) return null;

  const connection = {
    connecting: t.diagnosticsConnecting,
    connected: t.diagnosticsConnected,
    disconnected: t.diagnosticsDisconnected,
    failed: t.diagnosticsFailed,
    closed: t.diagnosticsClosed,
  }[diagnostics.peerState];
  const timing = (p50: number | null, p95: number | null) =>
    `${p50 ?? "—"} / ${p95 ?? "—"} ${t.durationUnit}`;
  const latestIds = [
    diagnostics.lastCapturedFrameId,
    diagnostics.lastSentFrameId,
    diagnostics.lastReceivedFrameId,
    diagnostics.lastRenderedFrameId,
  ].map((id) => id ?? "—").join(" / ");
  const rows = [
    [t.framesCaptured, diagnostics.framesCaptured.toLocaleString()],
    [t.framesSent, diagnostics.framesSent.toLocaleString()],
    [t.framesReceived, diagnostics.framesReceived.toLocaleString()],
    [t.framesRendered, diagnostics.framesRendered.toLocaleString()],
    [t.bytesSent, bytes(diagnostics.bytesSent)],
    [t.bytesReceived, bytes(diagnostics.bytesReceived)],
    [t.latestFrameIds, latestIds],
    [t.captureDuration, timing(diagnostics.captureP50Us, diagnostics.captureP95Us)],
    [t.resizeDuration, timing(diagnostics.resizeP50Us, diagnostics.resizeP95Us)],
    [t.encodeDuration, timing(diagnostics.encodeP50Us, diagnostics.encodeP95Us)],
    [t.sendDuration, timing(diagnostics.sendP50Us, diagnostics.sendP95Us)],
    [t.reassemblyDuration, timing(diagnostics.reassemblyP50Us, diagnostics.reassemblyP95Us)],
    [t.eventDuration, timing(diagnostics.eventP50Us, diagnostics.eventP95Us)],
    [t.backlogSkips, diagnostics.backlogSkips.toLocaleString()],
    [t.droppedFrames, `${diagnostics.incompleteFramesDropped} / ${diagnostics.staleChunksDropped}`],
    [t.inputFlow, `${diagnostics.inputsSent} / ${diagnostics.inputsDispatched} / ${diagnostics.inputsRejected}`],
    [t.acknowledgements, `${diagnostics.inputAcksSent} / ${diagnostics.inputAcksReceived}`],
    [t.diagnosticSamples, diagnostics.captureSamplesRetained.toLocaleString()],
  ];

  return (
    <details className="session-diagnostics">
      <summary>
        <Activity size={15} aria-hidden="true" />
        <span>{t.connectionDiagnostics}</span>
        <span className={`diagnostics-connection ${diagnostics.peerState === "failed" ? "is-failed" : ""}`}>
          {connection}
        </span>
      </summary>
      <dl>
        {rows.map(([label, value]) => (
          <div key={label}>
            <dt>{label}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
    </details>
  );
}
