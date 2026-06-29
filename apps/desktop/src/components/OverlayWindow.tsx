import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { PhysicalPosition, PhysicalSize } from "@tauri-apps/api/dpi";
import { api } from "../shared/api";
import { Icon } from "./Icon";
import type { RealtimeStateUpdate, TimelineSegment } from "../shared/types";
import { t } from "../i18n";

interface Update {
  projectId: string;
  sequence: number;
  segment: TimelineSegment;
  translation?: string;
  payload: Record<string, unknown>;
}

type RuntimeStatus = RealtimeStateUpdate["status"] | null;

export function OverlayWindow() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [projectId, setProjectId] = useState<string | null>(null);
  const [runtimeStatus, setRuntimeStatus] = useState<RuntimeStatus>(null);
  const [partial, setPartial] = useState("");
  const [showSource, setShowSource] = useState(true);
  const [showTranslation, setShowTranslation] = useState(false);
  const [fontSize, setFontSize] = useState(18);
  const [opacity, setOpacity] = useState(94);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const [actionBusy, setActionBusy] = useState(false);
  const [stopConfirm, setStopConfirm] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const settingTimers = useRef<Record<string, number | undefined>>({});
  const feedbackTimer = useRef<number | undefined>(undefined);
  const interruptedProject = useRef<string | null>(null);
  const guidance = useMemo(
    () => (update?.payload.guidance ?? {}) as Record<string, string[]>,
    [update],
  );

  const paused = runtimeStatus === "paused";
  const active = runtimeStatus === "running" || runtimeStatus === "paused";

  function showFeedback(message: string) {
    setFeedback(message);
    if (feedbackTimer.current !== undefined) {
      window.clearTimeout(feedbackTimer.current);
    }
    feedbackTimer.current = window.setTimeout(() => setFeedback(null), 4_000);
  }

  function scheduleSetting(key: string, value: unknown) {
    const current = settingTimers.current[key];
    if (current !== undefined) window.clearTimeout(current);
    settingTimers.current[key] = window.setTimeout(() => {
      delete settingTimers.current[key];
      void api.saveSetting(key, value).catch((error) => setErrorCode(String(error)));
    }, 250);
  }

  useEffect(() => {
    let disposers: Array<() => void> = [];
    void Promise.all([
      listen<Update>("accordmesh://realtime-understanding", (event) => {
        if (event.payload.projectId === interruptedProject.current) return;
        setErrorCode(null);
        setProjectId(event.payload.projectId);
        setUpdate((current) =>
          !current || event.payload.sequence >= current.sequence ? event.payload : current,
        );
      }),
      listen<{ projectId?: string; sourceTranscript: string }>("accordmesh://timeline-partial", (event) => {
        if (event.payload.projectId && event.payload.projectId === interruptedProject.current) return;
        setPartial(event.payload.sourceTranscript);
      }),
      listen<{ projectId?: string; errorCode?: string }>("accordmesh://realtime-error", (event) => {
        if (event.payload.projectId && event.payload.projectId === interruptedProject.current) return;
        setPartial(t("realtime.interrupted"));
        setErrorCode(event.payload.errorCode ?? "ERR_AUDIO_RUNTIME");
      }),
      listen<RealtimeStateUpdate>("accordmesh://realtime-state", (event) => {
        if (event.payload.status === "interrupted") {
          interruptedProject.current = event.payload.projectId;
          setUpdate(null);
          setProjectId(null);
          setRuntimeStatus(null);
          setPartial("");
          setErrorCode(null);
          setFeedback(null);
          setStopConfirm(false);
          void getCurrentWindow().hide();
          return;
        }
        interruptedProject.current = null;
        setProjectId(event.payload.projectId);
        setRuntimeStatus(event.payload.status);
        if (event.payload.status === "completed") setStopConfirm(false);
      }),
      listen<{ unlocked: boolean }>("accordmesh://vault-state", (event) => {
        if (event.payload.unlocked) return;
        setUpdate(null);
        setProjectId(null);
        setRuntimeStatus(null);
        setPartial("");
        setErrorCode(null);
        setFeedback(null);
        setStopConfirm(false);
        void getCurrentWindow().hide();
      }),
    ]).then((values) => {
      disposers = values;
    });
    void restoreWindow().catch(() => setErrorCode("ERR_OVERLAY"));
    return () => disposers.forEach((dispose) => dispose());
  }, []);

  useEffect(() => {
    const current = getCurrentWindow();
    const unlisten: Array<() => void> = [];
    void current
      .onMoved((event) =>
        scheduleSetting("overlayPosition", { x: event.payload.x, y: event.payload.y }),
      )
      .then((value) => unlisten.push(value));
    void current
      .onResized((event) =>
        scheduleSetting("overlaySize", {
          width: event.payload.width,
          height: event.payload.height,
        }),
      )
      .then((value) => unlisten.push(value));
    return () => {
      unlisten.forEach((value) => value());
      Object.values(settingTimers.current).forEach((timer) => {
        if (timer !== undefined) window.clearTimeout(timer);
      });
      if (feedbackTimer.current !== undefined) window.clearTimeout(feedbackTimer.current);
    };
  }, []);

  async function restoreWindow() {
    const [settings, currentState] = await Promise.all([
      api.loadSettings(),
      api.activeRealtimeState(),
    ]);
    const position = settings.overlayPosition as { x: number; y: number } | undefined;
    const size = settings.overlaySize as { width: number; height: number } | undefined;
    if (position) {
      await getCurrentWindow().setPosition(new PhysicalPosition(position.x, position.y));
    }
    if (size) {
      await getCurrentWindow().setSize(new PhysicalSize(size.width, size.height));
    }
    setFontSize(Number(settings.overlayFontSize ?? 18));
    setOpacity(Number(settings.overlayOpacity ?? 94));
    if (currentState) {
      setProjectId(currentState.projectId);
      setRuntimeStatus(currentState.status);
    }
  }

  async function togglePause() {
    if (!projectId || !active || actionBusy) return;
    setActionBusy(true);
    setErrorCode(null);
    try {
      if (paused) await api.resumeRealtime(projectId);
      else await api.pauseRealtime(projectId);
    } catch (error) {
      setErrorCode(String(error));
    } finally {
      setActionBusy(false);
    }
  }

  async function analyzeNow() {
    if (!projectId || runtimeStatus !== "running" || actionBusy) return;
    setActionBusy(true);
    setErrorCode(null);
    try {
      await api.analyzeNow(projectId);
      showFeedback(t("realtime.analyzeRequested"));
    } catch (error) {
      setErrorCode(String(error));
    } finally {
      setActionBusy(false);
    }
  }

  async function stopAssistance() {
    if (!projectId || !active || actionBusy) return;
    setActionBusy(true);
    setErrorCode(null);
    try {
      await api.stopRealtime(projectId);
      setRuntimeStatus("completed");
      setStopConfirm(false);
      await getCurrentWindow().hide();
    } catch (error) {
      const code = String(error);
      if (code === "ERR_REALTIME_STOP_TIMEOUT") {
        interruptedProject.current = projectId;
        setUpdate(null);
        setProjectId(null);
        setRuntimeStatus(null);
        setPartial("");
        setErrorCode(null);
        setFeedback(null);
        setStopConfirm(false);
        await getCurrentWindow().hide();
      } else {
        setErrorCode(code);
      }
    } finally {
      setActionBusy(false);
    }
  }

  async function copyContent() {
    try {
      await navigator.clipboard.writeText(
        [source, update?.translation, meaning].filter(Boolean).join("\n\n"),
      );
    } catch {
      setErrorCode("ERR_OVERLAY");
    }
  }

  const meaning = String(
    update?.payload.coreMeaning ??
      update?.payload.topicSummary ??
      t("realtime.waitingForSpeech"),
  );
  const source = update?.segment.sourceTranscript ?? partial;

  return (
    <main
      className="overlayWindow"
      style={{ fontSize, backgroundColor: `rgba(23,35,49,${opacity / 100})` }}
    >
      <header className="overlayHeader">
        <div className="overlayBrandLine">
          <span className="overlayBrandMark"><Icon name="brand" size={19} /></span>
          <div>
            <strong>{t("realtime.overlayTitle")}</strong>
            <span className="overlayRuntimeStatus">
              <i className={active ? "runtimeDot live" : "runtimeDot"} />
              {runtimeStatus === "completed"
                ? t("common.completed")
                : runtimeStatus === "interrupted"
                  ? t("realtime.interrupted")
                  : paused
                    ? t("common.paused")
                    : t("realtime.activeNotice")}
            </span>
          </div>
        </div>
        <button className="overlayCloseButton" title={t("common.hide")} aria-label={t("common.hide")} onClick={() => getCurrentWindow().hide()}>
          <Icon name="close" size={16} />
        </button>
      </header>
      {errorCode && (
        <div className="notice error" role="alert">
          {t(`errors.${errorCode}`)}
        </div>
      )}
      {feedback && (
        <div className="notice overlayFeedback" role="status" aria-live="polite">
          {feedback}
        </div>
      )}
      {showSource && source && (
        <section className="overlayInsightCard sourceCard">
          <span>{t("project.sourceTranscript")}</span>
          <p>{source}</p>
        </section>
      )}
      {showTranslation && update?.translation && (
        <section className="overlayInsightCard translationCard">
          <span>{t("analysis.literalTranslation")}</span>
          <p>{update.translation}</p>
        </section>
      )}
      <section className="overlayInsightCard meaningCard">
        <span>{t("realtime.coreMeaning")}</span>
        <p>{meaning}</p>
      </section>
      {(["answer", "explain", "ask", "confirm"] as const).map((key) =>
        guidance[key]?.length ? (
          <section className="overlayInsightCard guidanceCard" key={key}>
            <span>{t(`realtime.${key}`)}</span>
            <ul>
              {guidance[key].map((item) => (
                <li key={item}>{item}</li>
              ))}
            </ul>
          </section>
        ) : null,
      )}
      <div className="overlayUtilityControls">
        <button onClick={() => setShowSource(!showSource)}>
          {showSource ? t("realtime.hideSource") : t("realtime.viewSource")}
        </button>
        <button
          disabled={!update?.translation}
          onClick={() => setShowTranslation(!showTranslation)}
        >
          {t("realtime.translateSource")}
        </button>
        <button onClick={() => void copyContent()}>
          <Icon name="copy" size={15} />
          {t("common.copy")}
        </button>
      </div>
      <div className="overlayControls">
        <button
          className="overlayAction analyzeAction"
          disabled={runtimeStatus !== "running" || actionBusy}
          onClick={() => void analyzeNow()}
        >
          <Icon name="analyze" size={17} />
          {t("realtime.analyzeNow")}
        </button>
        <button className="overlayAction" disabled={!active || actionBusy} onClick={() => void togglePause()}>
          <Icon name={paused ? "resume" : "pause"} size={17} />
          {paused ? t("common.resume") : t("common.pause")}
        </button>
        <button
          className="overlayAction dangerButton stopAction"
          disabled={!active || actionBusy}
          onClick={() => setStopConfirm(true)}
        >
          <Icon name="stop" size={17} />
          {t("realtime.stop")}
        </button>
      </div>
      {stopConfirm && (
        <div className="overlayStopConfirm" role="alertdialog" aria-modal="true">
          <strong>{t("realtime.stopConfirmTitle")}</strong>
          <p>{t("realtime.stopConfirmBody")}</p>
          <div className="buttonRow">
            <button
              className="dangerButton"
              disabled={actionBusy}
              onClick={() => void stopAssistance()}
            >
              {t("realtime.stopAndFinalize")}
            </button>
            <button disabled={actionBusy} onClick={() => setStopConfirm(false)}>
              {t("common.cancel")}
            </button>
          </div>
        </div>
      )}
      <div className="overlaySliders">
        <label>
          {t("settings.overlayFontSize")}
          <input
            type="range"
            min="14"
            max="28"
            value={fontSize}
            onChange={(event) => {
              const value = Number(event.target.value);
              setFontSize(value);
              scheduleSetting("overlayFontSize", value);
            }}
          />
        </label>
        <label>
          {t("settings.overlayTransparency")}
          <input
            type="range"
            min="60"
            max="100"
            value={opacity}
            onChange={(event) => {
              const value = Number(event.target.value);
              setOpacity(value);
              scheduleSetting("overlayOpacity", value);
            }}
          />
        </label>
      </div>
    </main>
  );
}
