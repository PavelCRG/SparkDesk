import type { FormEvent, RefObject } from "react";
import { ArrowUpRight, Check, Copy, Keyboard, Link2, LoaderCircle, MonitorUp, ShieldCheck } from "lucide-react";
import { Badge, Button, Card, Heading, IconButton, Text, TextField, Tooltip } from "@radix-ui/themes";
import type { Translation } from "../copy";

type DeviceCardProps = {
  deviceId: string;
  copied: boolean;
  t: Translation;
  onCopy: () => void;
};

export function DeviceCard({ deviceId, copied, t, onCopy }: DeviceCardProps) {
  return (
    <Card className="device-card" variant="surface">
      <div className="card-heading">
        <span className="icon-tile icon-tile-blue"><MonitorUp size={18} /></span>
        <div><Text className="card-kicker">{t.yourDevice}</Text><Heading as="h2">{t.deviceId}</Heading></div>
        <Badge variant="soft" color="green" radius="full" className="secure-badge"><ShieldCheck size={12} /> PRIVATE</Badge>
      </div>
      <Text className="card-caption">{t.deviceId}</Text>
      <div className="device-id-row">
        <code>{deviceId || "···· ···· ···· ···· ···· ····"}</code>
        <Tooltip content={copied ? t.copied : t.copyId}>
          <IconButton className="copy-button" variant="soft" color={copied ? "green" : "gray"} aria-label={t.copyId} onClick={onCopy} disabled={!deviceId}>
            {copied ? <Check size={17} /> : <Copy size={17} />}
          </IconButton>
        </Tooltip>
      </div>
      <div className="device-footnote"><ShieldCheck size={14} />{t.safetyBody}</div>
    </Card>
  );
}

type ConnectCardProps = {
  inputRef: RefObject<HTMLInputElement | null>;
  peerId: string;
  fieldError: string;
  pending: boolean;
  sessionActive: boolean;
  t: Translation;
  onPeerIdChange: (value: string) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
};

export function ConnectCard({ inputRef, peerId, fieldError, pending, sessionActive, t, onPeerIdChange, onSubmit }: ConnectCardProps) {
  return (
    <Card className="connect-card" variant="surface">
      <div className="card-heading">
        <span className="icon-tile icon-tile-green"><Link2 size={18} /></span>
        <div><Text className="card-kicker">SPARKDESK CONNECT</Text><Heading as="h2">{t.connectTitle}</Heading></div>
      </div>
      <Text className="card-caption connect-caption">{t.connectDescription}</Text>
      <form className="connect-form" onSubmit={onSubmit}>
        <label htmlFor="partner-id">{t.partnerId}</label>
        <TextField.Root
          id="partner-id"
          name="partner-id"
          type="text"
          ref={inputRef}
          className="partner-input"
          size="3"
          value={peerId}
          onChange={(event) => onPeerIdChange(event.target.value)}
          placeholder={t.placeholder}
          autoComplete="off"
          spellCheck={false}
          aria-invalid={Boolean(fieldError)}
          aria-describedby="partner-id-error"
        />
        <Button className="connect-button" size="3" type="submit" disabled={pending || sessionActive}>
          {pending ? <LoaderCircle className="spin" size={17} /> : <ArrowUpRight size={17} />}
          {pending ? t.waiting : t.connect}
        </Button>
        <div className="form-error" id="partner-id-error" role="status" aria-live="polite">{fieldError}</div>
      </form>
      <div className="form-bottom"><span><Keyboard size={13} /> Ctrl+L</span><span>Enter <ArrowUpRight size={12} /></span></div>
    </Card>
  );
}
