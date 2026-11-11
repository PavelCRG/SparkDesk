import { forwardRef, type ReactNode } from "react";
import { Activity, Check, Maximize2, Minimize2, ShieldCheck, Timer } from "lucide-react";
import { Button, Heading, Switch } from "@radix-ui/themes";
import type { Translation } from "../copy";
import type { SessionDiagnostics, SessionRole } from "../types";
import { SessionDiagnosticsPanel } from "./SessionDiagnosticsPanel";

type SessionPageProps = {
  role: SessionRole;
  controlEnabled: boolean;
  detail: string;
  fullscreen: boolean;
  frameRate: number | null;
  inputLatencyP50: number | null;
  inputLatencyP95: number | null;
  diagnostics: SessionDiagnostics | null;
  t: Translation;
  children: ReactNode;
  onDisconnect: () => void;
  onToggleControl: (enabled: boolean) => void;
  onToggleFullscreen: () => void;
};

export const SessionPage = forwardRef<HTMLHeadingElement, SessionPageProps>(function SessionPage(
  { role, controlEnabled, detail, fullscreen, frameRate, inputLatencyP50, inputLatencyP95, diagnostics, t, children, onDisconnect, onToggleControl, onToggleFullscreen },
  ref,
) {
  const host = role === "host";
  return (
    <main id="session" className={`session-page ${host ? "is-host" : "is-viewer"}`}>
      <header className="session-toolbar">
        <div className="session-heading">
          <span className="session-connected"><span className="status-dot" />{t.connected}</span>
          <Heading as="h1" ref={ref} tabIndex={-1}>{host ? t.hostSession : t.viewerSession}</Heading>
          <p>{detail || t.connected}</p>
        </div>
        <div className="session-actions">
          {host && (
            <label className={`control-permission ${controlEnabled ? "is-enabled" : ""}`}>
              <span>{controlEnabled ? t.controlOn : t.remoteControl}</span>
              <Switch checked={controlEnabled} onCheckedChange={onToggleControl} aria-label={controlEnabled ? t.blockRemoteControl : t.remoteControl} />
            </label>
          )}
          {role === "viewer" && <span className={`control-state ${controlEnabled ? "is-enabled" : ""}`}>{controlEnabled ? t.controlOn : t.controlOff}</span>}
          {role === "viewer" && (
            <div className="session-metrics" aria-label={t.sessionPerformance}>
              <span title={t.framesPerSecond}><Activity size={14} />{frameRate ?? "—"} {t.framesPerSecond}</span>
              <span title={t.inputLatencyP50}>p50 {inputLatencyP50 ?? "—"} ms</span>
              <span title={t.inputLatencyP95}><Timer size={14} />p95 {inputLatencyP95 ?? "—"} ms</span>
            </div>
          )}
          {!host && <Button variant="soft" onClick={onToggleFullscreen} aria-label={fullscreen ? t.exitFullscreen : t.fullscreen}>{fullscreen ? <Minimize2 size={16} /> : <Maximize2 size={16} />}{fullscreen ? t.exitFullscreen : t.fullscreen}</Button>}
          <Button variant="soft" color="red" onClick={onDisconnect}>{t.endSession}</Button>
        </div>
      </header>
      {host ? (
        <section className="host-session-card" aria-label={t.hostSession}>
          <span className="host-session-icon"><ShieldCheck size={22} /></span>
          <div><strong>{t.hostSession}</strong><p>{controlEnabled ? t.controlOn : t.controlOff}</p></div>
          <Check className="host-session-check" size={17} />
        </section>
      ) : children}
      <SessionDiagnosticsPanel diagnostics={diagnostics} t={t} />
    </main>
  );
});
