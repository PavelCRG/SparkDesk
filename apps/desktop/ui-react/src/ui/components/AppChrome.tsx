import { ArrowDownLeft, Globe2, Minus, Square, X } from "lucide-react";
import { Badge, IconButton, Tooltip } from "@radix-ui/themes";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useMemo } from "react";
import type { Language, Translation } from "../copy";
import type { NetworkState } from "../types";

type AppHeaderProps = {
  language: Language;
  networkState: NetworkState;
  sessionActive: boolean;
  t: Translation;
  onLanguageChange: () => void;
};

export function AppHeader({ language, networkState, sessionActive, t, onLanguageChange }: AppHeaderProps) {
  const appWindow = useMemo(() => isTauri() ? getCurrentWindow() : null, []);
  const networkLabel: Record<NetworkState, string> = {
    not_configured: t.notConfigured,
    connecting: t.serverConnecting,
    online: t.online,
    reconnecting: t.reconnecting,
  };

  return (
    <header className="topbar" data-tauri-drag-region>
      <a className="brand" href={sessionActive ? "#session" : "#home"} aria-label="SparkDesk" data-tauri-drag-region>
        <span className="brand-mark"><img src="/sparkdesk-mark.png" alt="" /></span>
        <span className="brand-name">SparkDesk</span>
      </a>
      <div className="topbar-actions">
        <Badge className={`network-badge state-${networkState}`} variant="soft" radius="full">
          <span className="status-dot" />{networkLabel[networkState]}
        </Badge>
        <Tooltip content={language === "ru" ? t.switchToEnglish : t.switchToRussian}>
          <IconButton className="language-button" variant="ghost" color="gray" aria-label={t.switchLanguage} onClick={onLanguageChange}>
            <Globe2 size={16} /><span>{language.toUpperCase()}</span>
          </IconButton>
        </Tooltip>
        <div className="window-controls" role="group" aria-label={t.windowControls} data-tauri-drag-region>
          <Tooltip content={t.minimizeWindow}>
            <IconButton className="window-control" variant="ghost" aria-label={t.minimizeWindow} onClick={() => void appWindow?.minimize()}>
              <Minus size={15} />
            </IconButton>
          </Tooltip>
          <Tooltip content={t.toggleWindowSize}>
            <IconButton className="window-control" variant="ghost" aria-label={t.toggleWindowSize} onClick={() => void appWindow?.toggleMaximize()}>
              <Square size={13} />
            </IconButton>
          </Tooltip>
          <Tooltip content={t.closeWindow}>
            <IconButton className="window-control close-control" variant="ghost" aria-label={t.closeWindow} onClick={() => void appWindow?.close()}>
              <X size={16} />
            </IconButton>
          </Tooltip>
        </div>
      </div>
    </header>
  );
}

export function AppFooter({ t }: { t: Translation }) {
  return (
    <footer className="app-footer">
      <span className="footer-brand"><img src="/sparkdesk-mark.png" alt="" />SPARKDESK <i>·</i> PRIVATE BY DESIGN</span>
      <span><ArrowDownLeft size={13} /> {t.help}</span>
    </footer>
  );
}
