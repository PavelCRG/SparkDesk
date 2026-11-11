import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Heading, Text } from "@radix-ui/themes";
import { AppFooter, AppHeader } from "./components/AppChrome";
import { ConnectCard, DeviceCard } from "./components/ConnectionCards";
import { SessionFeedback } from "./components/SessionFeedback";
import { SessionPage } from "./components/SessionPage";
import { RemoteViewer } from "./components/RemoteViewer";
import { TrustStrip } from "./components/TrustStrip";
import { copy, type Language } from "./copy";
import type { AppStatus, InputAck, RemoteInput } from "./types";

const emptyStatus: AppStatus = { deviceId: "", networkState: "not_configured", noticeState: "idle", sessionRole: "none", controlEnabled: false, detail: "" };

function initialLanguage(): Language {
  const saved = window.localStorage.getItem("sparkdesk-language");
  return saved === "en" || saved === "ru" ? saved : "ru";
}

export function App() {
  const [language, setLanguage] = useState<Language>(initialLanguage);
  const [status, setStatus] = useState(emptyStatus);
  const [statusLoaded, setStatusLoaded] = useState(false);
  const [sessionWindowClosed, setSessionWindowClosed] = useState(false);
  const [peerId, setPeerId] = useState("");
  const [partnerError, setPartnerError] = useState("");
  const [message, setMessage] = useState("");
  const [frameRate, setFrameRate] = useState<number | null>(null);
  const [inputLatencyP95, setInputLatencyP95] = useState<number | null>(null);
  const [copied, setCopied] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const appWindow = useMemo(() => isTauri() ? getCurrentWindow() : null, []);
  const isSessionWindow = appWindow?.label === "session";
  const partnerInput = useRef<HTMLInputElement>(null);
  const nextInputSequence = useRef(0);
  const pendingInputs = useRef(new Map<number, number>());
  const inputRtts = useRef<number[]>([]);
  const page = useRef<HTMLHeadingElement>(null);
  const t = copy[language];
  const sessionActive = status.noticeState === "connected";
  const pending = status.noticeState === "pending";
  const previousSession = useRef(sessionActive);

  useEffect(() => {
    document.documentElement.lang = language;
    window.localStorage.setItem("sparkdesk-language", language);
  }, [language]);

  useEffect(() => {
    const onStorage = (event: StorageEvent) => {
      if (event.key === "sparkdesk-language" && (event.newValue === "en" || event.newValue === "ru")) {
        setLanguage(event.newValue);
      }
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);

  useEffect(() => {
    if (!appWindow) return;
    let unlisten: (() => void) | undefined;
    let alive = true;
    void listen<Language>("language-changed", (event) => {
      if (event.payload === "ru" || event.payload === "en") setLanguage(event.payload);
    }).then((stop) => {
      unlisten = stop;
      if (!alive) stop();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [appWindow]);

  useEffect(() => {
    if (!appWindow) return;
    let unlisten: (() => void) | undefined;
    let alive = true;
    void listen<InputAck>("input-ack", ({ payload }) => {
      const sentAt = pendingInputs.current.get(payload.sequence);
      if (sentAt === undefined) return;
      pendingInputs.current.delete(payload.sequence);
      if (!payload.applied) return;
      inputRtts.current.push(performance.now() - sentAt);
      if (inputRtts.current.length > 200) inputRtts.current.shift();
      const sorted = [...inputRtts.current].sort((a, b) => a - b);
      setInputLatencyP95(Math.round(sorted[Math.ceil(sorted.length * 0.95) - 1]));
    }).then((stop) => {
      unlisten = stop;
      if (!alive) stop();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [appWindow]);

  useEffect(() => {
    if (appWindow) void appWindow.isFullscreen().then(setFullscreen).catch(() => undefined);
  }, [appWindow]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let alive = true;

    void listen<AppStatus>("app-status", (event) => {
      setStatus(event.payload);
      setStatusLoaded(true);
    }).then((stop) => {
      unlisten = stop;
      if (!alive) stop();
    });
    void invoke<AppStatus>("get_app_status").then((current) => {
      if (alive) {
        setStatus(current);
        setStatusLoaded(true);
      }
    }).catch(() => {
      if (alive) setMessage(t.noServer);
    });

    return () => {
      alive = false;
      unlisten?.();
    };
  }, [t.noServer]);

  useEffect(() => {
    if (!appWindow || isSessionWindow) return;
    let unlisten: (() => void) | undefined;
    let alive = true;
    void listen("session-window-closed", () => setSessionWindowClosed(true)).then((stop) => {
      unlisten = stop;
      if (!alive) stop();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [appWindow, isSessionWindow]);

  useEffect(() => {
    if (!statusLoaded) return;
    if (!sessionActive) {
      setSessionWindowClosed(false);
      if (isSessionWindow) void appWindow?.close();
      return;
    }
    if (appWindow && !isSessionWindow && !sessionWindowClosed) {
      void invoke("open_session_window").catch((error) => {
        setMessage(typeof error === "string" ? error : t.requestError);
      });
    }
  }, [appWindow, isSessionWindow, sessionActive, sessionWindowClosed, statusLoaded, t.requestError]);

  useEffect(() => {
    document.title = sessionActive ? `${t.connected} · SparkDesk` : `${t.title} · SparkDesk`;
    if (isSessionWindow && previousSession.current !== sessionActive) page.current?.focus();
    previousSession.current = sessionActive;
  }, [isSessionWindow, sessionActive, t.connected, t.title]);

  useEffect(() => {
    if (!sessionActive && fullscreen) {
      void appWindow?.setFullscreen(false).then(() => setFullscreen(false)).catch(() => undefined);
    }
  }, [appWindow, fullscreen, sessionActive]);

  useEffect(() => {
    if (sessionActive) return;
    pendingInputs.current.clear();
    inputRtts.current = [];
    setFrameRate(null);
    setInputLatencyP95(null);
  }, [sessionActive]);

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
    const sequence = (nextInputSequence.current + 1) >>> 0;
    nextInputSequence.current = sequence;
    pendingInputs.current.set(sequence, performance.now());
    while (pendingInputs.current.size > 256) {
      const oldest = pendingInputs.current.keys().next().value;
      if (oldest === undefined) break;
      pendingInputs.current.delete(oldest);
    }
    void invoke("send_remote_input", { sequence, input }).catch(() => pendingInputs.current.delete(sequence));
  }, []);

  const reportFrameRate = useCallback((value: number) => setFrameRate(value), []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (isSessionWindow && sessionActive && event.key === "F11") {
        event.preventDefault();
        void toggleFullscreen();
      } else if (isSessionWindow && sessionActive && fullscreen && event.key === "Escape") {
        event.preventDefault();
        void toggleFullscreen();
      } else if (!isSessionWindow && event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "c") {
        event.preventDefault();
        void copyId();
      } else if (!isSessionWindow && event.ctrlKey && event.key.toLowerCase() === "l") {
        event.preventDefault();
        partnerInput.current?.focus();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [copyId, fullscreen, isSessionWindow, sessionActive, toggleFullscreen]);

  const switchLanguage = useCallback(() => {
    const next = language === "ru" ? "en" : "ru";
    setLanguage(next);
    if (appWindow) void emit("language-changed", next);
  }, [appWindow, language]);

  return (
    <div className={`app-shell ${fullscreen ? "is-fullscreen" : ""}`}>
      {!fullscreen && <AppHeader
        language={language}
        networkState={status.networkState}
        sessionActive={sessionActive && isSessionWindow}
        t={t}
        onLanguageChange={switchLanguage}
      />}

      {sessionActive && isSessionWindow ? (
        <SessionPage
          ref={page}
          role={status.sessionRole}
          controlEnabled={status.controlEnabled}
          detail={status.detail}
          fullscreen={fullscreen}
          frameRate={frameRate}
          inputLatencyP95={inputLatencyP95}
          t={t}
          onDisconnect={() => void disconnect()}
          onToggleControl={(enabled) => void toggleRemoteControl(enabled)}
          onToggleFullscreen={() => void toggleFullscreen()}
        >
          {status.sessionRole === "viewer" && <RemoteViewer controlEnabled={status.controlEnabled} fullscreen={fullscreen} t={t} onInput={sendRemoteInput} onFrameRate={reportFrameRate} />}
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
