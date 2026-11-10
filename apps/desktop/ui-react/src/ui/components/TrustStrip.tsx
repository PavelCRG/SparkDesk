import { ShieldCheck } from "lucide-react";
import type { Translation } from "../copy";

export function TrustStrip({ t }: { t: Translation }) {
  return (
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
  );
}
