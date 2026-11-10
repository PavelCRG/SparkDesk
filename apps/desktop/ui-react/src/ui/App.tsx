import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowDownLeft,
  ArrowUpRight,
  Check,
  CircleHelp,
  Copy,
  Globe2,
  Keyboard,
  Link2,
  LoaderCircle,
  MonitorUp,
  ShieldCheck,
  X,
} from "lucide-react";
import {
  Badge,
  Button,
  Card,
  Dialog,
  Flex,
  Heading,
  IconButton,
  Text,
  TextField,
  Tooltip,
} from "@radix-ui/themes";

type Language = "ru" | "en";

type AppStatus = {
  deviceId: string;
  networkState: number;
  noticeState: number;
  detail: string;
};

const copy = {
  en: {
    tagline: "REMOTE SUPPORT, WITH PERMISSION",
    title: "A clearer way to help.",
    intro: "Share your device ID or enter a partner’s. Every session starts with their approval.",
    yourDevice: "THIS DEVICE",
    deviceId: "Your SparkDesk ID",
    copyId: "Copy device ID",
    copied: "Copied",
    connectTitle: "Start a session",
    connectDescription: "Enter the 24-character ID of the device you want to reach.",
    partnerId: "Partner device ID",
    placeholder: "e.g. 4F9A02D1C7B85E30A4639D2A",
    connect: "Connect",
    connecting: "Sending request",
    requestSent: "Request sent",
    waiting: "Waiting for approval",
    connected: "Direct connection established",
    endSession: "End session",
    disconnected: "Session ended",
    offline: "Offline",
    online: "Ready to connect",
    serverConnecting: "Connecting to network",
    reconnecting: "Reconnecting",
    notConfigured: "Network not configured",
    consentTitle: "Connection request",
    consentBody: "wants to connect to this device. Allow this session?",
    accept: "Allow connection",
    decline: "Decline",
    safetyTitle: "You’re in control",
    safetyBody: "Your ID helps someone find this device. It never grants access on its own.",
    shortcut: "Shortcuts",
    shortcutCopy: "Copy ID",
    shortcutFocus: "Enter partner ID",
    invalidId: "Enter a valid 24-character device ID.",
    sameDevice: "That is this device’s own ID.",
    requestError: "Could not start the connection request.",
    noServer: "Set SPARKDESK_SIGNALING_URL before starting SparkDesk.",
    help: "Connection help",
  },
  ru: {
    tagline: "УДАЛЁННАЯ ПОМОЩЬ — С ВАШЕГО РАЗРЕШЕНИЯ",
    title: "Помогать стало проще.",
    intro: "Поделитесь своим ID или введите ID партнёра. Каждый сеанс начинается с его согласия.",
    yourDevice: "ЭТО УСТРОЙСТВО",
    deviceId: "Ваш ID SparkDesk",
    copyId: "Скопировать ID устройства",
    copied: "Скопировано",
    connectTitle: "Начать сеанс",
    connectDescription: "Введите 24-значный ID устройства, к которому хотите подключиться.",
    partnerId: "ID устройства партнёра",
    placeholder: "Например, 4F9A02D1C7B85E30A4639D2A",
    connect: "Подключиться",
    connecting: "Отправляем запрос",
    requestSent: "Запрос отправлен",
    waiting: "Ожидаем подтверждения",
    connected: "Прямое соединение установлено",
    endSession: "Завершить сеанс",
    disconnected: "Сеанс завершён",
    offline: "Не в сети",
    online: "Готово к подключению",
    serverConnecting: "Подключаемся к сети",
    reconnecting: "Восстанавливаем связь",
    notConfigured: "Сеть не настроена",
    consentTitle: "Запрос на подключение",
    consentBody: "хочет подключиться к этому устройству. Разрешить сеанс?",
    accept: "Разрешить подключение",
    decline: "Отклонить",
    safetyTitle: "Вы управляете доступом",
    safetyBody: "По ID можно найти устройство, но он сам по себе не даёт доступ к нему.",
    shortcut: "Сочетания клавиш",
    shortcutCopy: "Скопировать ID",
    shortcutFocus: "Ввести ID партнёра",
    invalidId: "Введите корректный ID из 24 символов.",
    sameDevice: "Это ID данного устройства.",
    requestError: "Не удалось отправить запрос на подключение.",
    noServer: "Перед запуском SparkDesk задайте SPARKDESK_SIGNALING_URL.",
    help: "Помощь с подключением",
  },
} satisfies Record<Language, Record<string, string>>;

const emptyStatus: AppStatus = { deviceId: "", networkState: 0, noticeState: 0, detail: "" };

export function App() {
  const [language, setLanguage] = useState<Language>("ru");
  const [status, setStatus] = useState(emptyStatus);
  const [peerId, setPeerId] = useState("");
  const [message, setMessage] = useState("");
  const [copied, setCopied] = useState(false);
  const partnerInput = useRef<HTMLInputElement>(null);
  const t = copy[language];

  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

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

  const decide = useCallback(async (accepted: boolean) => {
    try {
      await invoke("decide", { accepted });
    } catch {
      setMessage(t.requestError);
    }
  }, [t.requestError]);

  const submit = useCallback(async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const normalized = peerId.trim().toUpperCase();
    if (!/^[0-9A-F]{24}$/.test(normalized)) {
      setMessage(t.invalidId);
      return;
    }
    if (normalized === status.deviceId) {
      setMessage(t.sameDevice);
      return;
    }
    setMessage("");
    try {
      await invoke("connect", { peerId: normalized });
    } catch (error) {
      setMessage(typeof error === "string" ? error : t.requestError);
    }
  }, [peerId, status.deviceId, t.invalidId, t.requestError, t.sameDevice]);

  const copyId = useCallback(async () => {
    try {
      await invoke("copy_device_id");
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      setMessage(language === "ru" ? "Не удалось скопировать ID." : "Could not copy the device ID.");
    }
  }, [language]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "c") {
        event.preventDefault();
        void copyId();
      } else if (event.ctrlKey && event.key.toLowerCase() === "l") {
        event.preventDefault();
        partnerInput.current?.focus();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [copyId]);

  const networkLabel = [t.notConfigured, t.serverConnecting, t.online, t.reconnecting][status.networkState] ?? t.offline;
  const sessionActive = status.noticeState === 9;
  const pending = status.noticeState === 8;
  const incoming = status.noticeState === 3;

  return (
    <div className="app-shell">
      <header className="topbar">
        <a className="brand" href="#home" aria-label="SparkDesk home">
          <span className="brand-mark"><img src="/sparkdesk-mark.png" alt="" /></span>
          <span className="brand-name">SparkDesk</span>
        </a>
        <div className="topbar-actions">
          <Badge className={`network-badge state-${status.networkState}`} variant="soft" radius="full">
            <span className="status-dot" />{networkLabel}
          </Badge>
          <Tooltip content={language === "ru" ? "Switch to English" : "Переключить на русский"}>
            <IconButton className="language-button" variant="ghost" color="gray" aria-label="Switch language" onClick={() => setLanguage(language === "ru" ? "en" : "ru")}>
              <Globe2 size={16} /><span>{language.toUpperCase()}</span>
            </IconButton>
          </Tooltip>
        </div>
      </header>

      <main id="home" className="main-content">
        <section className="intro">
          <div className="eyebrow"><span className="eyebrow-rule" />{t.tagline}</div>
          <Heading as="h1" className="hero-title">{t.title}</Heading>
          <Text as="p" className="hero-copy">{t.intro}</Text>
        </section>

        <section className="connection-grid" aria-label={t.connectTitle}>
          <Card className="device-card" variant="surface">
            <div className="card-heading">
              <span className="icon-tile icon-tile-blue"><MonitorUp size={18} /></span>
              <div><Text className="card-kicker">{t.yourDevice}</Text><Heading as="h2">{t.deviceId}</Heading></div>
              <Badge variant="soft" color="green" radius="full" className="secure-badge"><ShieldCheck size={12} /> PRIVATE</Badge>
            </div>
            <Text className="card-caption">{t.deviceId}</Text>
            <div className="device-id-row">
              <code>{status.deviceId || "···· ···· ···· ···· ···· ····"}</code>
              <Tooltip content={copied ? t.copied : t.copyId}>
                <IconButton className="copy-button" variant="soft" color={copied ? "green" : "gray"} aria-label={t.copyId} onClick={() => void copyId()} disabled={!status.deviceId}>
                  {copied ? <Check size={17} /> : <Copy size={17} />}
                </IconButton>
              </Tooltip>
            </div>
            <div className="device-footnote"><ShieldCheck size={14} />{t.safetyBody}</div>
          </Card>

          <Card className="connect-card" variant="surface">
            <div className="card-heading">
              <span className="icon-tile icon-tile-green"><Link2 size={18} /></span>
              <div><Text className="card-kicker">SPARKDESK CONNECT</Text><Heading as="h2">{t.connectTitle}</Heading></div>
            </div>
            <Text className="card-caption connect-caption">{t.connectDescription}</Text>
            <form className="connect-form" onSubmit={(event) => void submit(event)}>
              <label htmlFor="partner-id">{t.partnerId}</label>
              <TextField.Root
                id="partner-id"
                ref={partnerInput}
                className="partner-input"
                size="3"
                value={peerId}
                onChange={(event) => setPeerId(event.target.value.toUpperCase().replace(/[^0-9A-F]/g, "").slice(0, 24))}
                placeholder={t.placeholder}
                autoComplete="off"
                spellCheck={false}
                aria-label={t.partnerId}
              />
              <Button className="connect-button" size="3" type="submit" disabled={!peerId || pending || sessionActive}>
                {pending ? <LoaderCircle className="spin" size={17} /> : <ArrowUpRight size={17} />}
                {pending ? t.waiting : t.connect}
              </Button>
            </form>
            <div className="form-bottom"><span><Keyboard size={13} /> Ctrl+L</span><span>Enter <ArrowUpRight size={12} /></span></div>
          </Card>
        </section>

        {(message || status.noticeState === 4 || pending || sessionActive) && (
          <section className={`session-banner ${sessionActive ? "is-connected" : pending ? "is-pending" : "is-error"}`} role="status" aria-live="polite">
            <span className="banner-icon">{sessionActive ? <ShieldCheck size={18} /> : pending ? <LoaderCircle className="spin" size={18} /> : <CircleHelp size={18} />}</span>
            <div className="banner-copy">
              <strong>{message || (sessionActive ? t.connected : pending ? t.waiting : status.detail || t.requestError)}</strong>
              {sessionActive && status.detail && <span>{status.detail}</span>}
            </div>
            {sessionActive && <Button variant="soft" color="red" onClick={() => void invoke("disconnect").catch(() => setMessage(t.requestError))}><X size={15} />{t.endSession}</Button>}
            {message && <IconButton variant="ghost" color="gray" aria-label="Dismiss message" onClick={() => setMessage("")}><X size={15} /></IconButton>}
          </section>
        )}

        <section className="trust-strip">
          <span className="trust-icon"><ShieldCheck size={19} /></span>
          <div className="trust-copy"><strong>{t.safetyTitle}</strong><span>{t.safetyBody}</span></div>
          <div className="trust-divider" />
          <div className="shortcut-list">
            <strong>{t.shortcut}</strong>
            <span><kbd>Ctrl</kbd><kbd>Shift</kbd><kbd>C</kbd> {t.shortcutCopy}</span>
            <span><kbd>Ctrl</kbd><kbd>L</kbd> {t.shortcutFocus}</span>
          </div>
        </section>
      </main>

      <footer className="app-footer"><span className="footer-brand"><img src="/sparkdesk-mark.png" alt="" />SPARKDESK <i>·</i> PRIVATE BY DESIGN</span><span><ArrowDownLeft size={13} /> {t.help}</span></footer>

      <Dialog.Root open={incoming} onOpenChange={(open) => { if (!open && incoming) void decide(false); }}>
        <Dialog.Content className="consent-dialog" maxWidth="420px">
          <div className="dialog-icon"><ArrowDownLeft size={21} /></div>
          <Dialog.Title>{t.consentTitle}</Dialog.Title>
          <Dialog.Description>{status.detail} {t.consentBody}</Dialog.Description>
          <Flex gap="3" mt="5" justify="end">
            <Button variant="soft" color="gray" onClick={() => void decide(false)}><X size={16} />{t.decline}</Button>
            <Button color="green" onClick={() => void decide(true)}><Check size={16} />{t.accept}</Button>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>
    </div>
  );
}
