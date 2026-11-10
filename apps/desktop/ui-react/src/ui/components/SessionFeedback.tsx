import { ArrowDownLeft, Check, CircleHelp, LoaderCircle, ShieldCheck, X } from "lucide-react";
import { Button, Dialog, Flex, IconButton } from "@radix-ui/themes";
import type { Translation } from "../copy";
import type { AppStatus } from "../types";

type SessionFeedbackProps = {
  status: AppStatus;
  message: string;
  t: Translation;
  onDecide: (accepted: boolean) => void;
  onDisconnect: () => void;
  onDismiss: () => void;
};

export function SessionFeedback({ status, message, t, onDecide, onDisconnect, onDismiss }: SessionFeedbackProps) {
  const sessionActive = status.noticeState === "connected";
  const pending = status.noticeState === "pending";
  const incoming = status.noticeState === "incoming_request";
  const hasFeedback = Boolean(message) || status.noticeState === "error" || pending || sessionActive;

  return (
    <>
      {hasFeedback && (
        <section className={`session-banner ${sessionActive ? "is-connected" : pending ? "is-pending" : "is-error"}`} role="status" aria-live="polite">
          <span className="banner-icon">{sessionActive ? <ShieldCheck size={18} /> : pending ? <LoaderCircle className="spin" size={18} /> : <CircleHelp size={18} />}</span>
          <div className="banner-copy">
            <strong>{message || (sessionActive ? t.connected : pending ? t.waiting : status.detail || t.requestError)}</strong>
            {sessionActive && status.detail && <span>{status.detail}</span>}
          </div>
          {sessionActive && <Button variant="soft" color="red" onClick={onDisconnect}><X size={15} />{t.endSession}</Button>}
          {message && <IconButton variant="ghost" color="gray" aria-label={t.dismissMessage} onClick={onDismiss}><X size={15} /></IconButton>}
        </section>
      )}

      <Dialog.Root open={incoming} onOpenChange={(open) => { if (!open && incoming) onDecide(false); }}>
        <Dialog.Content className="consent-dialog" maxWidth="420px">
          <div className="dialog-icon"><ArrowDownLeft size={21} /></div>
          <Dialog.Title>{t.consentTitle}</Dialog.Title>
          <Dialog.Description>{status.detail} {t.consentBody}</Dialog.Description>
          <Flex gap="3" mt="5" justify="end">
            <Button variant="soft" color="gray" onClick={() => onDecide(false)}><X size={16} />{t.decline}</Button>
            <Button color="green" onClick={() => onDecide(true)}><Check size={16} />{t.accept}</Button>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>
    </>
  );
}
