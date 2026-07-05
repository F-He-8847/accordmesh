import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import type {
  AudioDeviceInfo,
  LanguagePreferences,
  ProviderConfigurationStatus,
  ProviderDefinition,
  RealtimeMode,
  SoundCheck,
  SystemAudioStatus,
} from "../../shared/types";
import { api } from "../../shared/api";
import {
  languageLabelKey,
  providerLanguageCompatibility,
  resolveLanguagePreferences,
} from "../../shared/languagePreferences";
import { t } from "../../i18n";
import {
  PROJECT_TITLE_MAX_CHARS,
  projectTitleLength,
} from "../../shared/projectTitle";

interface Props {
  mode: RealtimeMode;
  providers: ProviderDefinition[];
  providerStatuses: ProviderConfigurationStatus[];
  preferences: LanguagePreferences;
  defaultProviderId: string;
  onStart: (
    mode: RealtimeMode,
    title: string,
    deviceId: string,
    providerId: string,
  ) => Promise<void>;
  onError: (code: string | null) => void;
}

export function MeetingStartPage({
  mode,
  providers,
  providerStatuses,
  preferences,
  defaultProviderId,
  onStart,
  onError,
}: Props) {
  const [title, setTitle] = useState("");
  const [providerId, setProviderId] = useState(defaultProviderId);
  const [devices, setDevices] = useState<AudioDeviceInfo[]>([]);
  const [deviceId, setDeviceId] = useState("");
  const [check, setCheck] = useState<SoundCheck | null>(null);
  const [systemAudio, setSystemAudio] = useState<SystemAudioStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [loadingDevices, setLoadingDevices] = useState(false);
  const [checkingSound, setCheckingSound] = useState(false);
  const [requestingSystemAudio, setRequestingSystemAudio] = useState(false);
  const soundCheckRunRef = useRef(0);
  const isOnline = mode === "online";

  const provider = useMemo(
    () => providers.find((value) => value.id === providerId),
    [providerId, providers],
  );
  const configured =
    providerId === "mock" ||
    providerStatuses.some(
      (value) => value.providerId === providerId && value.configured,
    );
  const selectedDevice = devices.find((device) => device.id === deviceId);
  const resolved = resolveLanguagePreferences(preferences);
  const languageCompatibility = providerLanguageCompatibility(
    provider?.capabilities,
    preferences,
  );

  async function refreshDevices() {
    setCheck(null);
    setLoadingDevices(true);
    try {
      if (!api.isNative) {
        setDevices([
          {
            id: "mock-microphone",
            label: t("realtime.mockDevice"),
            isDefault: true,
            permissionStatus: "demo",
            available: true,
          },
        ]);
        setDeviceId("mock-microphone");
        setSystemAudio({
          available: true,
          supported: true,
          backend: "demo",
          permissionStatus: "demo",
          deviceLabel: t("realtime.mockDevice"),
          requiresRestart: false,
        });
        return;
      }
      const [inputs, system] = await Promise.all([
        api.audioDevices(),
        isOnline ? api.systemAudioStatus() : Promise.resolve(null),
      ]);
      setDevices(inputs);
      setDeviceId((current) =>
        inputs.some((device) => device.id === current)
          ? current
          : inputs.find((device) => device.isDefault)?.id ?? inputs[0]?.id ?? "",
      );
      setSystemAudio(system);
    } catch (error) {
      setDevices([]);
      setDeviceId("");
      onError(String(error));
    } finally {
      setLoadingDevices(false);
    }
  }

  useEffect(() => {
    void refreshDevices();
  }, [isOnline]);

  async function requestSystemAudioPermission() {
    setRequestingSystemAudio(true);
    try {
      setSystemAudio(await api.requestSystemAudioPermission());
    } catch (error) {
      onError(String(error));
    } finally {
      setRequestingSystemAudio(false);
    }
  }

  useEffect(() => {
    soundCheckRunRef.current += 1;
    setCheckingSound(false);
    setCheck(null);
    void api.cancelSoundCheck();
    return () => {
      soundCheckRunRef.current += 1;
      void api.cancelSoundCheck();
    };
  }, [deviceId, mode]);

  async function soundCheck() {
    if (!deviceId) return;
    const runId = soundCheckRunRef.current + 1;
    soundCheckRunRef.current = runId;
    setCheckingSound(true);
    setCheck(null);
    try {
      if (!api.isNative) {
        if (soundCheckRunRef.current === runId) {
          setCheck({
            level: 0.42,
            peak: 0.61,
            lowVolume: false,
            excessiveNoise: false,
            clipping: false,
            status: "ready",
          });
        }
        return;
      }
      const result = await api.soundCheck(deviceId);
      if (soundCheckRunRef.current === runId) setCheck(result);
    } catch (error) {
      const code = String(error);
      if (
        soundCheckRunRef.current === runId &&
        code !== "ERR_AUDIO_CHECK_CANCELLED"
      ) {
        onError(code);
      }
    } finally {
      if (soundCheckRunRef.current === runId) setCheckingSound(false);
    }
  }

  async function cancelSoundCheck() {
    soundCheckRunRef.current += 1;
    setCheckingSound(false);
    setCheck(null);
    try {
      await api.cancelSoundCheck();
    } catch (error) {
      onError(String(error));
    }
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (projectTitleLength(title) > PROJECT_TITLE_MAX_CHARS) {
      onError("ERR_TITLE_TOO_LONG");
      return;
    }
    setBusy(true);
    try {
      await onStart(mode, title, deviceId, providerId);
    } catch (error) {
      onError(String(error));
    } finally {
      setBusy(false);
    }
  }

  const titleLength = projectTitleLength(title);
  const titleOverLimit = titleLength > PROJECT_TITLE_MAX_CHARS;
  const systemReady =
    !isOnline || providerId === "mock" || systemAudio?.available === true;
  const canStart = Boolean(
    title.trim() &&
      !titleOverLimit &&
      deviceId &&
      selectedDevice?.available &&
      provider?.capabilities.realtimeTranscription &&
      configured &&
      systemReady &&
      languageCompatibility.supported &&
      !busy &&
      !loadingDevices &&
      !checkingSound &&
      !requestingSystemAudio,
  );
  const readinessKey = canStart
    ? "realtime.readyToStart"
    : isOnline && providerId !== "mock" && !systemReady
      ? "realtime.systemAudioRequiredBeforeStart"
      : "realtime.completeRequiredFields";

  return (
    <section className="meetingStartPage">
      <div className="pageHeader">
        <div>
          <h1>{isOnline ? t("realtime.onlineTitle") : t("realtime.inPersonTitle")}</h1>
          <p>{t("realtime.privacyNotice")}</p>
        </div>
      </div>
      <form className="meetingStartForm" onSubmit={submit}>
        <label className="meetingField">
          <span>{t("common.provider")}</span>
          <select value={providerId} onChange={(event) => setProviderId(event.target.value)}>
            {providers.map((value) => (
              <option key={value.id} value={value.id}>
                {t(value.displayNameKey)}
              </option>
            ))}
          </select>
          {providerId === "openai" && configured && (
            <small>{t("providers.configuredNotVerified")}</small>
          )}
        </label>
        {!configured && <div className="notice error">{t("providers.configurationRequired")}</div>}
        {!languageCompatibility.supported && languageCompatibility.errorCode && (
          <div className="notice error">{t(`errors.${languageCompatibility.errorCode}`)}</div>
        )}
        {api.isNative && providerId === "mock" && (
          <div className="notice">{t("realtime.mockProviderAudioNote")}</div>
        )}
        <label className="meetingField">
          <span>{t("realtime.projectTitle")}</span>
          <input
            value={title}
            aria-invalid={titleOverLimit}
            onChange={(event) => setTitle(event.target.value)}
            required
          />
          <small className={`characterCount ${titleOverLimit ? "isOverLimit" : ""}`}>
            {t("realtime.titleCharacterCount", {
              count: titleLength,
              max: PROJECT_TITLE_MAX_CHARS,
            })}
          </small>
          {titleOverLimit && <small className="fieldError">{t("errors.ERR_TITLE_TOO_LONG")}</small>}
        </label>

        {isOnline && (
          <div className="meetingDeviceCard systemAudioCard" data-ready={systemReady}>
            <div className="meetingCardHeader">
              <div>
                <span className="meetingFieldLabel">{t("realtime.systemAudio")}</span>
                <strong>{systemAudio?.deviceLabel ?? t("common.loading")}</strong>
              </div>
              <span className={`meetingStatusBadge ${systemReady ? "ready" : "attention"}`}>
                {systemReady ? t("realtime.deviceReady") : t("realtime.permissionRequired")}
              </span>
            </div>
            <p>
              {providerId === "mock"
                ? t("realtime.mockSystemAudioBypass")
                : systemAudio?.available
                  ? t("realtime.systemAudioReadyDescription")
                  : t("realtime.systemAudioPermissionDescription")}
            </p>
            {systemAudio?.requiresRestart && (
              <div className="notice">{t("realtime.systemAudioRestartRequired")}</div>
            )}
            {providerId !== "mock" && !systemAudio?.available && (
              <div className="buttonRow meetingActionRow">
                <button
                  type="button"
                  onClick={() => void requestSystemAudioPermission()}
                  disabled={requestingSystemAudio}
                >
                  {requestingSystemAudio
                    ? t("common.processing")
                    : t("realtime.requestSystemAudioPermission")}
                </button>
                <button type="button" onClick={() => void api.openSystemAudioSettings()}>
                  {t("realtime.openSystemSettings")}
                </button>
                <button type="button" onClick={() => void refreshDevices()}>
                  {t("realtime.checkAgain")}
                </button>
              </div>
            )}
          </div>
        )}

        <div className="meetingDeviceCard">
          <div className="meetingCardHeader">
            <strong>{isOnline ? t("realtime.microphone") : t("realtime.roomMicrophone")}</strong>
            <button
              type="button"
              onClick={() => void refreshDevices()}
              disabled={loadingDevices || checkingSound}
            >
              {loadingDevices ? t("common.loading") : t("realtime.refreshDevices")}
            </button>
          </div>
          {devices.length ? (
            <>
              <label className="meetingField">
                <span>{t("realtime.inputDevice")}</span>
                <select
                  value={deviceId}
                  onChange={(event) => {
                    setDeviceId(event.target.value);
                    setCheck(null);
                  }}
                >
                  {devices.map((device) => (
                    <option value={device.id} key={device.id}>
                      {device.label}
                      {device.isDefault ? ` (${t("realtime.defaultDevice")})` : ""}
                    </option>
                  ))}
                </select>
              </label>
              {selectedDevice && (
                <p className="meetingDeviceMeta">
                  {t("realtime.deviceDetails", {
                    permission: selectedDevice.permissionStatus,
                    rate: selectedDevice.sampleRate ?? "-",
                    channels: selectedDevice.channels ?? "-",
                  })}
                </p>
              )}
              <button
                type="button"
                onClick={() => void (checkingSound ? cancelSoundCheck() : soundCheck())}
                disabled={!deviceId || loadingDevices}
              >
                {checkingSound ? t("realtime.cancelSoundCheck") : t("realtime.runSoundCheck")}
              </button>
              {check && (
                <div className="soundCheckResult">
                  <meter min={0} max={1} value={check.level} />
                  <span>{t(`realtime.soundStatus.${check.status}`)}</span>
                </div>
              )}
            </>
          ) : (
            <div className="notice error">{t("realtime.noMicrophones")}</div>
          )}
        </div>

        <div className="meetingPreferenceSummary">
          <div className="meetingPreferenceItem">
            <span>{t("realtime.sourceLanguageMode")}</span>
            <strong>
              {preferences.sourceLanguageMode === "auto"
                ? t("realtime.autoDetect")
                : t(languageLabelKey(resolved.sourceLanguage ?? "en"))}
            </strong>
          </div>
          <div className="meetingPreferenceItem">
            <span>{t("common.translationLanguage")}</span>
            <strong>
              {resolved.translationTargetLanguage
                ? t(languageLabelKey(resolved.translationTargetLanguage))
                : t("settings.noTranslation")}
            </strong>
          </div>
          <div className="meetingPreferenceItem">
            <span>{t("common.outputLanguage")}</span>
            <strong>{t(languageLabelKey(resolved.analysisOutputLanguage))}</strong>
          </div>
        </div>

        <div className={`meetingReadinessNotice ${canStart ? "ready" : "pending"}`}>
          {t(readinessKey)}
        </div>
        <button className="primaryButton meetingStartButton" disabled={!canStart}>
          {busy
            ? t("common.processing")
            : isOnline
              ? t("realtime.startOnline")
              : t("realtime.startInPerson")}
        </button>
      </form>
    </section>
  );
}
