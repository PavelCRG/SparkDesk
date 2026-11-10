import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Heading, Text } from "@radix-ui/themes";
import { AppFooter, AppHeader } from "./components/AppChrome";
import { ConnectCard, DeviceCard } from "./components/ConnectionCards";
import { SessionFeedback } from "./components/SessionFeedback";
import { SessionPage } from "./components/SessionPage";
import { RemoteViewer } from "./components/RemoteViewer";
import { TrustStrip } from "./components/TrustStrip";
import { copy, type Language } from "./copy";
import type { AppStatus, RemoteInput } from "./types";

const emptyStatus: AppStatus = { deviceId: "", networkState: "not_configured", noticeState: "idle", sessionRole: "none", controlEnabled: false, detail: "" };

export function App() {
  const [language, setLanguage] = useState<Language>("ru");
  const [status, setStatus] = useState(emptyStatus);
  const [peerId, setPeerId] = useState("");
  const [partnerError, setPartnerError] = useState("");
  const [message, setMessage] = useState("");
  const [copied, setCopied] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const appWindow = useMemo(() => isTauri() ? getCurrentWindow() : null, []);
  const partnerInput = useRef<HTMLInputElement>(null);
  const page = useRef<HTMLHeadingElement>(null);
  const t = copy[language];
  const sessionActive = status.noticeState === "connected";
  const pending = status.noticeState === "pending";
  const previousSession = useRef(sessionActive);

  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

  useEffect(() => {
    if (appWindow) void appWindow.isFullscreen().then(setFullscreen).catch(() => undefined);
  }, [appWindow]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let alive = true;

    void listen<AppStatus>("app-status", (event) => setStatus(event.payload)).then((stop) => {
      unlisten = stop;
      if (!alive) stop();
    });
    void invoke<AppStatus>("get_app_status").then((current) => {
      if (alive) setStatus(current);
    }).catch(() => {
      if (alive) setMessage(t.noServer);
    });

    return () => {
      alive = false;
      unlisten?.();
    };
  }, [t.noServer]);

  useEffect(() => {
    document.title = sessionActive ? `${t.connected} · SparkDesk` : `${t.title} · SparkDesk`;
    if (previousSession.current !== sessionActive) page.current?.focus();
    previousSession.current = sessionActive;
  }, [sessionActive, t.connected, t.title]);

  useEffect(() => {
    if (!sessionActive && fullscreen) {
      void appWindow?.setFullscreen(false).then(() => setFullscreen(false)).catch(() => undefined);
    }
  }, [appWindow, fullscreen, sessionActive]);

  const toggleFullscreen = useCallback(async () => {
    if (!appWindow) return;
    try {
      const next = !(await appWindow.isFullscreen());
      await appWindow.setFullscreen(next);
      setFullscreen(next);
    } catch {
      setMessage(t.requestError);
    }
  }, [appWindow, t.requestError]);

  const decide = useCallback(async (accepted: boolean) => {
    try {
      await invoke("decide", { accepted });
    } catch {
      setMessage(t.requestError);
    }
  }, [t.requestError]);

  const submit = useCallback(async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const normalized = peerId.trim().toUpperCase();
    if (!/^[0-9A-F]{24}$/.test(normalized)) {
      setPartnerError(t.invalidId);
      setMessage(t.invalidId);
      partnerInput.current?.focus();
      return;
    }
    if (normalized === status.deviceId) {
      setPartnerError(t.sameDevice);
      setMessage(t.sameDevice);
      partnerInput.current?.focus();
      return;
    }
    setPartnerError("");
    setMessage("");
    try {
      await invoke("connect", { peerId: normalized });
    } catch (error) {
      setMessage(typeof error === "string" ? error : t.requestError);
    }
  }, [peerId, status.deviceId, t.invalidId, t.requestError, t.sameDevice]);

  const updatePeerId = useCallback((value: string) => {
    setPeerId(value.toUpperCase().replace(/[^0-9A-F]/g, "").slice(0, 24));
    setPartnerError("");
    setMessage("");
  }, []);

  const copyId = useCallback(async () => {
    try {
      await invoke("copy_device_id");
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      setMessage(language === "ru" ? "Не удалось скопировать ID." : "Could not copy the device ID.");
    }
  }, [language]);

  const disconnect = useCallback(async () => {
    try {
      await invoke("disconnect");
    } catch {
      setMessage(t.requestError);
    }
  }, [t.requestError]);

  const toggleRemoteControl = useCallback(async (enabled: boolean) => {
    try {
      await invoke("set_remote_control", { enabled });
    } catch {
      setMessage(t.requestError);
    }
  }, [t.requestError]);

  const sendRemoteInput = useCallback((input: RemoteInput) => {
    void invoke("send_remote_input", { input }).catch(() => undefined);
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (sessionActive && event.key === "F11") {
        event.preventDefault();
        void toggleFullscreen();
      } else if (sessionActive && fullscreen && event.key === "Escape") {
        event.preventDefault();
        void toggleFullscreen();
      } else if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "c") {
        event.preventDefault();
        void copyId();
      } else if (event.ctrlKey && event.key.toLowerCase() === "l") {
        event.preventDefault();
        partnerInput.current?.focus();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [copyId, fullscreen, sessionActive, toggleFullscreen]);

  return (
    <div className={`app-shell ${fullscreen ? "is-fullscreen" : ""}`}>
      {!fullscreen && <AppHeader
        language={language}
        networkState={status.networkState}
        sessionActive={sessionActive}
        t={t}
        onLanguageChange={() => setLanguage(language === "ru" ? "en" : "ru")}
      />}

      {sessionActive ? (
        <SessionPage
          ref={page}
          role={status.sessionRole}
          controlEnabled={status.controlEnabled}
          detail={status.detail}
          fullscreen={fullscreen}
          t={t}
          onDisconnect={() => void disconnect()}
          onToggleControl={(enabled) => void toggleRemoteControl(enabled)}
          onToggleFullscreen={() => void toggleFullscreen()}
        >
          {status.sessionRole === "viewer" && <RemoteViewer controlEnabled={status.controlEnabled} fullscreen={fullscreen} t={t} onInput={sendRemoteInput} />}
        </SessionPage>
      ) : (
        <main id="home" className="main-content">
          <section className="intro">
            <div className="eyebrow"><span className="eyebrow-rule" />{t.tagline}</div>
            <Heading as="h1" ref={page} tabIndex={-1} className="hero-title">{t.title}</Heading>
            <Text as="p" className="hero-copy">{t.intro}</Text>
          </section>

          <section className="connection-grid" aria-label={t.connectTitle}>
            <DeviceCard deviceId={status.deviceId} copied={copied} t={t} onCopy={() => void copyId()} />
            <ConnectCard
              inputRef={partnerInput}
              peerId={peerId}
              fieldError={partnerError}
              pending={pending}
              sessionActive={sessionActive}
              t={t}
              onPeerIdChange={updatePeerId}
              onSubmit={(event) => void submit(event)}
            />
          </section>

          <SessionFeedback
            status={status}
            message={message}
            t={t}
            onDecide={(accepted) => void decide(accepted)}
            onDisconnect={() => void disconnect()}
            onDismiss={() => setMessage("")}
            onToggleControl={(enabled) => void toggleRemoteControl(enabled)}
          />
          <TrustStrip t={t} />
        </main>
      )}

      {!sessionActive && <AppFooter t={t} />}
    </div>
  );
}
