import React, { useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./styles.css";

type CaptureResult = {
  text: string;
  source: string;
};

type StatusPayload = {
  status: "idle" | "busy" | "error" | string;
  message: string;
};

type OverlayPayload = {
  preview: string;
};

type SpeakOptions = {
  voice: string;
  rate: string;
  volume: string;
};

const defaultOptions: SpeakOptions = {
  voice: "zh-CN-XiaoxiaoNeural",
  rate: "+0%",
  volume: "+0%",
};

const copy = {
  detected: "\u68c0\u6d4b\u5230\u53ef\u6717\u8bfb\u6587\u672c",
  overlayReading: "\u6b63\u5728\u6717\u8bfb\u5f53\u524d\u6587\u672c\u6bb5",
  overlayClick: "\u70b9\u51fb\u6717\u8bfb\u5f53\u524d\u6587\u672c\u6bb5",
  playCurrent: "\u64ad\u653e\u5f53\u524d\u6587\u672c",
  playing: "\u505c\u6b62",
  clickRead: "\u70b9\u51fb\u6717\u8bfb",
  stopReading: "\u505c\u6b62\u6717\u8bfb",
  ready: "\u51c6\u5907\u5c31\u7eea\u3002\u628a\u9f20\u6807\u60ac\u505c\u5728\u5176\u4ed6\u5e94\u7528\u7684\u6587\u672c\u6bb5\u4e0a\uff0c\u51fa\u73b0\u8986\u76d6\u6587\u672c\u7684\u534a\u900f\u660e\u8fb9\u6846\u540e\u70b9\u51fb\u64ad\u653e\u3002",
  readingCursor: "\u6b63\u5728\u8bfb\u53d6\u9f20\u6807\u4e0b\u65b9\u6587\u672c...",
  readDone: "\u5df2\u8bfb\u53d6\uff1a",
  readingAndSpeaking: "\u6b63\u5728\u8bfb\u53d6\u5e76\u6717\u8bfb\u9f20\u6807\u4e0b\u65b9\u6587\u672c...",
  speakingInput: "\u6b63\u5728\u6717\u8bfb\u8f93\u5165\u6587\u672c...",
  title: "\u60ac\u505c\u8bc6\u522b\u6587\u672c\uff0c\u70b9\u51fb\u8fb9\u6846\u6717\u8bfb",
  subtitle: "\u9f20\u6807\u60ac\u505c\u5230\u5176\u4ed6\u5e94\u7528\u7684\u53ef\u8bbf\u95ee\u6587\u672c\u6bb5\u4e0a\u65b9\u65f6\uff0cReadit \u4f1a\u5728\u6587\u672c\u6bb5\u843d\u672c\u8eab\u4e0a\u663e\u793a\u534a\u900f\u660e\u8fb9\u6846\u906e\u7f69\uff1b\u70b9\u51fb\u8986\u76d6\u5c42\u5373\u53ef\u6717\u8bfb\u3002",
  debugSpeak: "\u8c03\u8bd5\uff1a\u8bfb\u53d6\u5e76\u6717\u8bfb\u9f20\u6807\u4e0b\u6587\u672c",
  captureOnly: "\u4ec5\u8bfb\u53d6\u6587\u672c",
  mainInteraction: "\u4e3b\u4ea4\u4e92\uff1a\u60ac\u505c\u6587\u672c\u6bb5 \u2192 \u70b9\u51fb\u8986\u76d6\u8fb9\u6846\u64ad\u653e",
  ttsSettings: "TTS \u8bbe\u7f6e",
  installHint: "\u9996\u6b21\u4f7f\u7528\u524d\u8bf7\u8fd0\u884c\uff1a",
  recent: "\u6700\u8fd1\u8bfb\u53d6",
  textareaPlaceholder: "\u8bfb\u53d6\u5230\u7684\u6587\u672c\u4f1a\u663e\u793a\u5728\u8fd9\u91cc\uff0c\u4e5f\u53ef\u4ee5\u624b\u52a8\u8f93\u5165\u6587\u672c\u6717\u8bfb\u3002",
  speakTextarea: "\u6717\u8bfb\u6587\u672c\u6846\u5185\u5bb9",
  mvpBoundary: "MVP \u8fb9\u754c",
  limit1: "\u652f\u6301\u66b4\u9732 UI Automation \u6587\u672c\u7684\u5e94\u7528\uff0c\u4f8b\u5982\u6d4f\u89c8\u5668\u666e\u901a\u7f51\u9875\u3001\u90e8\u5206 PDF \u9605\u8bfb\u5668\u3001\u8bb0\u4e8b\u672c\u3002",
  limit2: "\u56fe\u7247\u578b PDF\u3001Canvas\u3001\u8fdc\u7a0b\u684c\u9762\u3001\u6e38\u620f\u548c\u81ea\u7ed8\u63a7\u4ef6\u53ef\u80fd\u65e0\u6cd5\u8bfb\u53d6\uff0c\u540e\u7eed\u53ef\u52a0 OCR \u964d\u7ea7\u3002",
  limit3: "\u4f7f\u7528 edge-tts \u65f6\uff0c\u5f85\u6717\u8bfb\u6587\u672c\u4f1a\u53d1\u9001\u5230\u5728\u7ebf\u8bed\u97f3\u670d\u52a1\uff0c\u8bf7\u907f\u514d\u6717\u8bfb\u654f\u611f\u5185\u5bb9\u3002",
};

const isOverlayWindow = new URLSearchParams(window.location.search).get("overlay") === "1";

function OverlayApp() {
  const [preview, setPreview] = useState(copy.detected);
  const [playing, setPlaying] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    const unlisteners: Array<() => void> = [];

    listen<OverlayPayload>("readit://overlay-hover", (event) => {
      setPreview(event.payload.preview || copy.detected);
      setError("");
      if (!playing) {
        setPlaying(false);
      }
    }).then((unlisten) => unlisteners.push(unlisten));

    listen<StatusPayload>("readit://status", (event) => {
      if (event.payload.status === "busy") {
        setPlaying(true);
        setError("");
      }
      if (event.payload.status === "idle") {
        setPlaying(false);
      }
      if (event.payload.status === "error") {
        setPlaying(false);
        setError(event.payload.message);
      }
    }).then((unlisten) => unlisteners.push(unlisten));

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [playing]);

  async function toggleHoverText() {
    try {
      setError("");
      if (playing) {
        await invoke("stop_speaking");
        setPlaying(false);
        return;
      }

      setPlaying(true);
      await invoke("speak_hover_text", { options: defaultOptions });
    } catch (err) {
      setPlaying(false);
      setError(String(err));
    }
  }

  return (
    <main
      className={`overlay-shell ${playing ? "is-playing" : ""}`}
      onClick={toggleHoverText}
      title={error || preview}
      aria-label={playing ? copy.stopReading : copy.overlayClick}
    >
      <div className="overlay-border" aria-hidden="true" />
      <button className="overlay-play" aria-label={playing ? copy.stopReading : copy.playCurrent}>
        {playing ? <span className="stop-square" /> : <span className="play-triangle" />}
      </button>
      <span className="overlay-hint">{playing ? copy.playing : copy.clickRead}</span>
    </main>
  );
}

function App() {
  const [options, setOptions] = useState<SpeakOptions>(defaultOptions);
  const [captured, setCaptured] = useState<CaptureResult | null>(null);
  const [manualText, setManualText] = useState("");
  const [status, setStatus] = useState<StatusPayload>({
    status: "idle",
    message: copy.ready,
  });

  const isBusy = status.status === "busy";
  const previewText = useMemo(() => captured?.text ?? "", [captured]);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];

    listen<StatusPayload>("readit://status", (event) => {
      setStatus(event.payload);
    }).then((unlisten) => unlisteners.push(unlisten));

    listen<{ text: string }>("readit://captured", (event) => {
      setCaptured({ text: event.payload.text, source: "readit" });
      setManualText(event.payload.text);
    }).then((unlisten) => unlisteners.push(unlisten));

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  async function capture() {
    try {
      setStatus({ status: "busy", message: copy.readingCursor });
      const result = await invoke<CaptureResult>("capture_text_under_cursor");
      setCaptured(result);
      setManualText(result.text);
      setStatus({ status: "idle", message: `${copy.readDone}${result.source}` });
    } catch (error) {
      setStatus({ status: "error", message: String(error) });
    }
  }

  async function speakUnderCursor() {
    try {
      setStatus({ status: "busy", message: copy.readingAndSpeaking });
      const result = await invoke<CaptureResult>("speak_text_under_cursor", { options });
      setCaptured(result);
      setManualText(result.text);
    } catch (error) {
      setStatus({ status: "error", message: String(error) });
    }
  }

  async function speakManualText() {
    try {
      setStatus({ status: "busy", message: copy.speakingInput });
      await invoke("speak_text", { text: manualText || previewText, options });
    } catch (error) {
      setStatus({ status: "error", message: String(error) });
    }
  }

  return (
    <main className="shell">
      <section className="hero">
        <div>
          <p className="eyebrow">Readit MVP ? Windows</p>
          <h1>{copy.title}</h1>
          <p className="subtitle">{copy.subtitle}</p>
        </div>
        <div className={`status ${status.status}`}>
          <span>{status.status}</span>
          <strong>{status.message}</strong>
        </div>
      </section>

      <section className="card actions">
        <button disabled={isBusy} onClick={speakUnderCursor}>
          {copy.debugSpeak}
        </button>
        <button disabled={isBusy} className="secondary" onClick={capture}>
          {copy.captureOnly}
        </button>
        <div className="shortcut">{copy.mainInteraction}</div>
      </section>

      <section className="grid">
        <div className="card">
          <h2>{copy.ttsSettings}</h2>
          <label>
            Voice
            <input
              value={options.voice}
              onChange={(event) => setOptions({ ...options, voice: event.target.value })}
              placeholder="zh-CN-XiaoxiaoNeural"
            />
          </label>
          <label>
            Rate
            <input
              value={options.rate}
              onChange={(event) => setOptions({ ...options, rate: event.target.value })}
              placeholder="+0%"
            />
          </label>
          <label>
            Volume
            <input
              value={options.volume}
              onChange={(event) => setOptions({ ...options, volume: event.target.value })}
              placeholder="+0%"
            />
          </label>
          <p className="hint">
            {copy.installHint}<code>python -m pip install edge-tts</code>
          </p>
        </div>

        <div className="card">
          <h2>{copy.recent}</h2>
          <textarea
            value={manualText || previewText}
            onChange={(event) => setManualText(event.target.value)}
            placeholder={copy.textareaPlaceholder}
          />
          <button disabled={isBusy || !(manualText || previewText).trim()} onClick={speakManualText}>
            {copy.speakTextarea}
          </button>
        </div>
      </section>

      <section className="card notes">
        <h2>{copy.mvpBoundary}</h2>
        <ul>
          <li>{copy.limit1}</li>
          <li>{copy.limit2}</li>
          <li>{copy.limit3}</li>
        </ul>
      </section>
    </main>
  );
}

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{isOverlayWindow ? <OverlayApp /> : <App />}</React.StrictMode>,
);
