import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent, type WheelEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import type { Translation } from "../copy";
import type { RemoteInput, ScreenFrame } from "../types";

type RemoteViewerProps = {
  controlEnabled: boolean;
  fullscreen: boolean;
  t: Translation;
  onInput: (input: RemoteInput) => void;
};

function point(event: PointerEvent<HTMLElement> | WheelEvent<HTMLElement>, image: HTMLImageElement | null) {
  const bounds = image?.getBoundingClientRect();
  const rect = bounds && bounds.width > 0 && bounds.height > 0 ? bounds : event.currentTarget.getBoundingClientRect();
  const sourceWidth = image?.naturalWidth || image?.width || rect.width;
  const sourceHeight = image?.naturalHeight || image?.height || rect.height;
  const scale = Math.min(rect.width / sourceWidth, rect.height / sourceHeight);
  const contentWidth = sourceWidth * scale;
  const contentHeight = sourceHeight * scale;
  const left = rect.left + (rect.width - contentWidth) / 2;
  const top = rect.top + (rect.height - contentHeight) / 2;
  const x = contentWidth ? (event.clientX - left) / contentWidth : 0;
  const y = contentHeight ? (event.clientY - top) / contentHeight : 0;
  return {
    x: Math.max(0, Math.min(65535, Math.round(x * 65535))),
    y: Math.max(0, Math.min(65535, Math.round(y * 65535))),
  };
}

export function RemoteViewer({ controlEnabled, fullscreen, t, onInput }: RemoteViewerProps) {
  const viewer = useRef<HTMLDivElement>(null);
  const image = useRef<HTMLImageElement>(null);
  const heldKeys = useRef(new Set<string>());
  const heldButtons = useRef(new Set<"left" | "right" | "middle" | "back" | "forward">());
  const lastPointer = useRef({ x: 0, y: 0 });
  const pendingPointer = useRef<{ x: number; y: number } | null>(null);
  const pointerFrame = useRef<number | null>(null);
  const [frame, setFrame] = useState<ScreenFrame | null>(null);

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    void listen<ScreenFrame>("screen-frame", (event) => {
      if (alive) setFrame(event.payload);
    }).then((stop) => {
      unlisten = stop;
      if (!alive) stop();
    });
    return () => {
      alive = false;
      unlisten?.();
      if (pointerFrame.current !== null) cancelAnimationFrame(pointerFrame.current);
    };
  }, []);

  const releaseInputs = () => {
    for (const code of heldKeys.current) onInput({ type: "key", code, down: false });
    heldKeys.current.clear();
    for (const button of heldButtons.current) {
      onInput({ type: "pointer_button", button, down: false, ...lastPointer.current });
    }
    heldButtons.current.clear();
  };

  const movePointer = (event: PointerEvent<HTMLDivElement>) => {
    if (!controlEnabled) return;
    pendingPointer.current = point(event, image.current);
    if (pointerFrame.current !== null) return;
    pointerFrame.current = requestAnimationFrame(() => {
      pointerFrame.current = null;
      const next = pendingPointer.current;
      if (next) {
        lastPointer.current = next;
        onInput({ type: "pointer_move", ...next });
      }
    });
  };

  useEffect(() => {
    if (!controlEnabled) {
      if (pointerFrame.current !== null) cancelAnimationFrame(pointerFrame.current);
      pointerFrame.current = null;
      pendingPointer.current = null;
      releaseInputs();
    }
  }, [controlEnabled]);

  const button = (event: PointerEvent<HTMLElement>, down: boolean) => {
    if (!controlEnabled) return;
    if (event.button < 0 || event.button > 4) return;
    if (event.button > 2) event.preventDefault();
    const buttonName = event.button === 4 ? "forward" : event.button === 3 ? "back" : event.button === 2 ? "right" : event.button === 1 ? "middle" : "left";
    lastPointer.current = point(event, image.current);
    if (down) heldButtons.current.add(buttonName);
    else heldButtons.current.delete(buttonName);
    onInput({ type: "pointer_button", button: buttonName, down, ...lastPointer.current });
  };

  const keyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.code === "F11" || (fullscreen && event.code === "Escape")) return;
    if (!controlEnabled || event.repeat || !event.code) return;
    event.preventDefault();
    heldKeys.current.add(event.code);
    onInput({ type: "key", code: event.code, down: true });
  };

  const keyUp = (event: KeyboardEvent<HTMLElement>) => {
    if (!controlEnabled || !heldKeys.current.delete(event.code)) return;
    event.preventDefault();
    onInput({ type: "key", code: event.code, down: false });
  };

  return (
    <section className="remote-viewer" aria-label={t.viewerSession}>
      <div
        ref={viewer}
        className={`viewer-screen ${controlEnabled ? "can-control" : ""}`}
        tabIndex={0}
        role="application"
        aria-label={t.screenHelp}
        onPointerDown={(event) => { event.currentTarget.focus(); event.currentTarget.setPointerCapture(event.pointerId); button(event, true); }}
        onPointerUp={(event) => button(event, false)}
        onPointerMove={movePointer}
        onPointerCancel={releaseInputs}
        onAuxClick={(event) => event.preventDefault()}
        onWheel={(event) => {
          if (!controlEnabled) return;
          event.preventDefault();
          const delta = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? 40 : event.deltaMode === WheelEvent.DOM_DELTA_PAGE ? 800 : 1;
          const coords = point(event, image.current);
          if (event.deltaY) onInput({ type: "pointer_wheel", delta: Math.max(-1200, Math.min(1200, Math.round(-event.deltaY * delta))), horizontal: false, ...coords });
          if (event.deltaX) onInput({ type: "pointer_wheel", delta: Math.max(-1200, Math.min(1200, Math.round(-event.deltaX * delta))), horizontal: true, ...coords });
        }}
        onKeyDown={keyDown}
        onKeyUp={keyUp}
        onBlur={releaseInputs}
        onContextMenu={(event) => event.preventDefault()}
      >
        {frame ? (
          <img
            ref={image}
            src={`data:image/jpeg;base64,${frame.jpegBase64}`}
            width={frame.width}
            height={frame.height}
            alt={t.viewerSession}
            draggable={false}
          />
        ) : <span className="viewer-placeholder">{t.screenWaiting}</span>}
      </div>
      {controlEnabled && <p className="viewer-help">{t.screenHelp}</p>}
    </section>
  );
}
