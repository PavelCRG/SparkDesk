import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent, type WheelEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import type { Translation } from "../copy";
import type { RemoteInput, ScreenFrame } from "../types";

type RemoteViewerProps = {
  controlEnabled: boolean;
  fullscreen: boolean;
  t: Translation;
  onInput: (input: RemoteInput) => void;
  onFrameRate: (framesPerSecond: number) => void;
};

function point(event: PointerEvent<HTMLElement> | WheelEvent<HTMLElement>, image: HTMLCanvasElement | null) {
  const bounds = image?.getBoundingClientRect();
  const rect = bounds && bounds.width > 0 && bounds.height > 0 ? bounds : event.currentTarget.getBoundingClientRect();
  const sourceWidth = image?.width || rect.width;
  const sourceHeight = image?.height || rect.height;
  const scale = Math.min(rect.width / sourceWidth, rect.height / sourceHeight);
  const contentWidth = sourceWidth * scale;
  const contentHeight = sourceHeight * scale;
  const left = rect.left + (rect.width - contentWidth) / 2;
  const top = rect.top + (rect.height - contentHeight) / 2;
  const x = contentWidth ? (event.clientX - left) / contentWidth : 0;
  const y = contentHeight ? (event.clientY - top) / contentHeight : 0;
  if (x < 0 || x > 1 || y < 0 || y > 1) return null;
  return {
    x: Math.round(x * 65535),
    y: Math.round(y * 65535),
  };
}

export function RemoteViewer({ controlEnabled, fullscreen, t, onInput, onFrameRate }: RemoteViewerProps) {
  const viewer = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const heldKeys = useRef(new Set<string>());
  const heldButtons = useRef(new Set<"left" | "right" | "middle" | "back" | "forward">());
  const lastPointer = useRef({ x: 0, y: 0 });
  const pendingPointer = useRef<{ x: number; y: number } | null>(null);
  const pointerFrame = useRef<number | null>(null);
  const renderedFrames = useRef({ startedAt: performance.now(), count: 0 });
  const hasRenderedFrame = useRef(false);
  const [hasFrame, setHasFrame] = useState(false);

  useEffect(() => {
    let alive = true;
    let drawing = false;
    let latestFrame: ScreenFrame | null = null;
    let unlisten: (() => void) | undefined;
    const drawLatestFrame = async () => {
      if (drawing || !alive) return;
      drawing = true;
      while (alive && latestFrame) {
        const frame = latestFrame;
        latestFrame = null;
        let bitmap: ImageBitmap;
        try {
          const binary = atob(frame.jpegBase64);
          const bytes = new Uint8Array(binary.length);
          for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
          bitmap = await createImageBitmap(new Blob([bytes], { type: "image/jpeg" }));
        } catch {
          continue;
        }
        if (!alive || latestFrame) {
          bitmap.close();
          continue;
        }
        await new Promise<void>((resolve) => requestAnimationFrame(() => {
          const target = canvas.current;
          if (alive && !latestFrame && target) {
            if (target.width !== bitmap.width) target.width = bitmap.width;
            if (target.height !== bitmap.height) target.height = bitmap.height;
            const context = target.getContext("2d");
            if (context) {
              context.drawImage(bitmap, 0, 0);
              if (!hasRenderedFrame.current) {
                hasRenderedFrame.current = true;
                setHasFrame(true);
              }
              recordRenderedFrame();
            }
          }
          bitmap.close();
          resolve();
        }));
      }
      drawing = false;
      if (alive && latestFrame) void drawLatestFrame();
    };
    void listen<ScreenFrame>("screen-frame", (event) => {
      latestFrame = event.payload;
      void drawLatestFrame();
    }).then((stop) => {
      unlisten = stop;
      if (!alive) stop();
    });
    return () => {
      alive = false;
      latestFrame = null;
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
    if (!controlEnabled || !hasRenderedFrame.current) return;
    const coordinates = point(event, canvas.current);
    if (!coordinates) return;
    pendingPointer.current = coordinates;
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
    if (!controlEnabled || !hasRenderedFrame.current) return;
    if (event.button < 0 || event.button > 4) return;
    if (event.button > 2) event.preventDefault();
    const buttonName = event.button === 4 ? "forward" : event.button === 3 ? "back" : event.button === 2 ? "right" : event.button === 1 ? "middle" : "left";
    const coordinates = point(event, canvas.current);
    if (down && !coordinates) return;
    if (coordinates) lastPointer.current = coordinates;
    if (down) heldButtons.current.add(buttonName);
    else heldButtons.current.delete(buttonName);
    onInput({ type: "pointer_button", button: buttonName, down, ...lastPointer.current });
  };

  const keyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.code === "F11" || (fullscreen && event.code === "Escape")) return;
    if (!controlEnabled || !hasRenderedFrame.current || event.repeat || !event.code) return;
    event.preventDefault();
    heldKeys.current.add(event.code);
    onInput({ type: "key", code: event.code, down: true });
  };

  const keyUp = (event: KeyboardEvent<HTMLElement>) => {
    if (!controlEnabled || !heldKeys.current.delete(event.code)) return;
    event.preventDefault();
    onInput({ type: "key", code: event.code, down: false });
  };

  const recordRenderedFrame = () => {
    const sample = renderedFrames.current;
    sample.count += 1;
    const now = performance.now();
    if (sample.count === 1) {
      sample.startedAt = now;
      return;
    }
    const elapsed = now - sample.startedAt;
    if (elapsed >= 1000) {
      onFrameRate(Math.round(sample.count * 1000 / elapsed));
      sample.startedAt = now;
      sample.count = 0;
    }
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
          if (!controlEnabled || !hasRenderedFrame.current) return;
          event.preventDefault();
          const delta = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? 40 : event.deltaMode === WheelEvent.DOM_DELTA_PAGE ? 800 : 1;
          const coords = point(event, canvas.current);
          if (!coords) return;
          if (event.deltaY) onInput({ type: "pointer_wheel", delta: Math.max(-1200, Math.min(1200, Math.round(-event.deltaY * delta))), horizontal: false, ...coords });
          if (event.deltaX) onInput({ type: "pointer_wheel", delta: Math.max(-1200, Math.min(1200, Math.round(-event.deltaX * delta))), horizontal: true, ...coords });
        }}
        onKeyDown={keyDown}
        onKeyUp={keyUp}
        onBlur={releaseInputs}
        onContextMenu={(event) => event.preventDefault()}
      >
        <canvas ref={canvas} width={1} height={1} role="img" aria-label={t.viewerSession} />
        {!hasFrame && <span className="viewer-placeholder">{t.screenWaiting}</span>}
      </div>
      {controlEnabled && <p className="viewer-help">{t.screenHelp}</p>}
    </section>
  );
}
