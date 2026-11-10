import { ArrowDownLeft, Globe2 } from "lucide-react";
import { Badge, IconButton, Tooltip } from "@radix-ui/themes";
import type { Language, Translation } from "../copy";

type AppHeaderProps = {
  language: Language;
  networkState: number;
  t: Translation;
  onLanguageChange: () => void;
};

export function AppHeader({ language, networkState, t, onLanguageChange }: AppHeaderProps) {
  const networkLabel = [t.notConfigured, t.serverConnecting, t.online, t.reconnecting][networkState] ?? t.offline;

  return (
    <header className="topbar">
      <a className="brand" href="#home" aria-label={t.home}>
        <span className="brand-mark"><img src="/sparkdesk-mark.png" alt="" /></span>
        <span className="brand-name">SparkDesk</span>
      </a>
      <div className="topbar-actions">
        <Badge className={`network-badge state-${networkState}`} variant="soft" radius="full">
          <span className="status-dot" />{networkLabel}
        </Badge>
        <Tooltip content={language === "ru" ? "Switch to English" : "Переключить на русский"}>
          <IconButton className="language-button" variant="ghost" color="gray" aria-label={t.switchLanguage} onClick={onLanguageChange}>
            <Globe2 size={16} /><span>{language.toUpperCase()}</span>
          </IconButton>
        </Tooltip>
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
