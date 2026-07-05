import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import { Dialog } from "../../components/Dialog";
import { ResetVaultDialog } from "../../components/ResetVaultDialog";
import { Icon } from "../../components/Icon";
import type { IconName } from "../../components/Icon";
import { t } from "../../i18n";
import { api } from "../../shared/api";
import { APP_VERSION } from "../../shared/appVersion";
import { ENABLE_DEV_TOOLS } from "../../shared/buildFlags";
import {
  canUseAsDefaultProvider,
  isUiOnlyProvider,
  providerDescriptionKey,
  providerSettingsPanelKind,
  visibleProviderDefinitions,
} from "../../shared/providerUiRegistry";
import {
  InactivityLockMinutes,
  LANGUAGE_CODES,
  NO_TRANSLATION,
  SAME_AS_ANALYSIS,
  SAME_AS_TRANSLATION,
  languageLabelKey,
  resolveLanguagePreferences,
} from "../../shared/languagePreferences";
import type {
  LanguagePreferences,
  MediaRuntimeStatus,
  ProviderConfigurationStatus,
  ProviderDefinition,
} from "../../shared/types";

interface Props {
  providers: ProviderDefinition[];
  providerStatuses: ProviderConfigurationStatus[];
  preferences: LanguagePreferences;
  defaultProviderId: string;
  inactivityMinutes: InactivityLockMinutes;
  onDefaultProvider: (value: string) => void;
  onInactivityMinutes: (value: InactivityLockMinutes) => void;
  onPreferences: (preferences: LanguagePreferences) => void;
  onRefreshStatuses: () => Promise<void>;
  onError: (code: string | null) => void;
}

type SettingsFeedback = {
  tone: "success" | "error";
  message: string;
};

type SettingsCategory =
  | "general"
  | "appearance"
  | "audio"
  | "language"
  | "provider"
  | "security"
  | "advanced"
  | "about";

const settingsCategories: Array<{
  id: SettingsCategory;
  icon: IconName;
}> = [
  { id: "general", icon: "settings" },
  { id: "appearance", icon: "media" },
  { id: "audio", icon: "inPerson" },
  { id: "language", icon: "comparison" },
  { id: "provider", icon: "analyze" },
  { id: "security", icon: "shield" },
  ...(ENABLE_DEV_TOOLS ? [{ id: "advanced" as const, icon: "sort" as const }] : []),
  { id: "about", icon: "brand" },
];

function providerDefault(
  definition: ProviderDefinition | undefined,
  id: string,
  fallback: string,
) {
  return (
    definition?.configurationSchema.find((field) => field.id === id)
      ?.defaultValue ?? fallback
  );
}

function configuredString(
  status: ProviderConfigurationStatus | undefined,
  id: string,
  fallback: string,
) {
  const value = status?.configuration?.[id];
  return typeof value === "string" && value.trim() ? value : fallback;
}

function languageOptions() {
  return LANGUAGE_CODES.map((code) => (
    <option value={code} key={code}>
      {t(languageLabelKey(code))}
    </option>
  ));
}

function providerStatusLabel(providerId: string, status: ProviderConfigurationStatus | undefined) {
  if (isUiOnlyProvider(providerId)) {
    return t("providers.status.ui_test_only");
  }
  if (providerId === "mock" && status?.configured) {
    return t("providers.status.local_ready");
  }
  if (providerId === "openai" && status?.maskedSummary === "ready") {
    return t("providers.status.saved_not_verified");
  }
  return t(`providers.status.${status?.maskedSummary ?? "not_configured"}`);
}

function providerStatusClass(
  providerId: string,
  status: ProviderConfigurationStatus | undefined,
) {
  if (isUiOnlyProvider(providerId)) return "statusWarning";
  if (providerId === "mock" && status?.configured) return "statusGood";
  if (providerId === "openai" && status?.configured) return "statusWarning";
  if (status?.stored) return "statusWarning";
  return "statusMuted";
}

function providerIsReady(
  providerId: string,
  statuses: ProviderConfigurationStatus[],
) {
  if (!canUseAsDefaultProvider(providerId)) return false;
  if (isUiOnlyProvider(providerId)) return ENABLE_DEV_TOOLS;
  return statuses.some(
    (status) => status.providerId === providerId && status.configured,
  );
}

function configuredModel(
  definition: ProviderDefinition,
  status: ProviderConfigurationStatus | undefined,
  fieldId: string,
) {
  const configured = status?.configuration?.[fieldId];
  if (typeof configured === "string" && configured.trim()) return configured;
  return (
    definition.configurationSchema.find((field) => field.id === fieldId)
      ?.defaultValue ?? t("providers.modelNotAssigned")
  );
}

const OVERLAY_FONT_SIZE_MIN = 14;
const OVERLAY_FONT_SIZE_MAX = 28;
const OVERLAY_OPACITY_MIN = 60;
const OVERLAY_OPACITY_MAX = 100;

function clampInteger(value: number, min: number, max: number) {
  if (!Number.isFinite(value)) return min;
  return Math.min(max, Math.max(min, Math.round(value)));
}

function parseIntegerDraft(
  value: string,
  fallback: number,
  min: number,
  max: number,
) {
  const trimmed = value.trim();
  if (!trimmed) return fallback;
  const parsed = Number(trimmed);
  if (!Number.isFinite(parsed)) return fallback;
  return clampInteger(parsed, min, max);
}

export function SettingsPage({
  providers,
  providerStatuses,
  preferences,
  defaultProviderId,
  inactivityMinutes,
  onDefaultProvider,
  onInactivityMinutes,
  onPreferences,
  onRefreshStatuses,
  onError,
}: Props) {
  const [activeCategory, setActiveCategory] =
    useState<SettingsCategory>("general");
  const [activeProviderId, setActiveProviderId] = useState(defaultProviderId || "openai");
  const [apiKey, setApiKey] = useState("");
  const [apiKeyVisible, setApiKeyVisible] = useState(false);
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com/v1");
  const [transcriptionModel, setTranscriptionModel] = useState(
    "gpt-4o-mini-transcribe",
  );
  const [analysisModel, setAnalysisModel] = useState("gpt-5-mini");
  const [mockScenario, setMockScenario] = useState("normal");
  const [overlayFontSize, setOverlayFontSize] = useState(18);
  const [overlayFontSizeInput, setOverlayFontSizeInput] = useState("18");
  const [overlayOpacity, setOverlayOpacity] = useState(94);
  const [overlayOpacityInput, setOverlayOpacityInput] = useState("94");
  const [feedback, setFeedback] = useState<SettingsFeedback | null>(null);
  const feedbackTimer = useRef<number | undefined>(undefined);
  const [providerBusy, setProviderBusy] = useState(false);
  const [mockBusy, setMockBusy] = useState(false);
  const [removeProviderOpen, setRemoveProviderOpen] = useState(false);
  const [removeApiKeyOpen, setRemoveApiKeyOpen] = useState(false);
  const [resetOpen, setResetOpen] = useState(false);
  const [mediaRuntime, setMediaRuntime] = useState<MediaRuntimeStatus | null>(null);
  const [mediaRuntimeLoading, setMediaRuntimeLoading] = useState(false);

  const openAiDefinition = providers.find((value) => value.id === "openai");
  const mockDefinition = providers.find((value) => value.id === "mock");
  const openAiStatus = providerStatuses.find(
    (value) => value.providerId === "openai",
  );
  const mockStatus = providerStatuses.find(
    (value) => value.providerId === "mock",
  );
  const openAiKeySaved =
    openAiStatus?.credentialFieldsConfigured.includes("apiKey") ?? false;
  const baseUrlDefault = providerDefault(
    openAiDefinition,
    "baseUrl",
    "https://api.openai.com/v1",
  );
  const transcriptionModelDefault = providerDefault(
    openAiDefinition,
    "transcriptionModel",
    "gpt-4o-mini-transcribe",
  );
  const analysisModelDefault = providerDefault(
    openAiDefinition,
    "analysisModel",
    "gpt-5-mini",
  );
  const mockScenarioDefault = providerDefault(
    mockDefinition,
    "scenario",
    "normal",
  );
  const resolved = useMemo(
    () => resolveLanguagePreferences(preferences),
    [preferences],
  );
  const hasLegacyCustomOpenAiEndpoint =
    baseUrl.trim().replace(/\/+$/, "") !== baseUrlDefault;
  const visibleProviders = useMemo(
    () => visibleProviderDefinitions(providers),
    [providers],
  );
  const defaultProviderOptions = useMemo(
    () => visibleProviders,
    [visibleProviders],
  );
  const activeProvider = useMemo(() => {
    return (
      visibleProviders.find((provider) => provider.id === activeProviderId) ??
      visibleProviders.find((provider) => provider.id === "openai") ??
      visibleProviders[0]
    );
  }, [activeProviderId, visibleProviders]);
  const activeProviderStatus = activeProvider
    ? providerStatuses.find((value) => value.providerId === activeProvider.id)
    : undefined;
  const activeProviderPanelKind = activeProvider
    ? providerSettingsPanelKind(activeProvider.id)
    : "uiExtensionTest";

  function dismissFeedback() {
    if (feedbackTimer.current !== undefined) {
      window.clearTimeout(feedbackTimer.current);
      feedbackTimer.current = undefined;
    }
    setFeedback(null);
  }

  function showFeedback(
    message: string,
    tone: SettingsFeedback["tone"] = "success",
  ) {
    if (feedbackTimer.current !== undefined) {
      window.clearTimeout(feedbackTimer.current);
    }
    setFeedback({ message, tone });
    feedbackTimer.current = window.setTimeout(() => {
      setFeedback(null);
      feedbackTimer.current = undefined;
    }, 4_000);
  }

  useEffect(() => {
    return () => {
      if (feedbackTimer.current !== undefined) {
        window.clearTimeout(feedbackTimer.current);
      }
    };
  }, []);

  useEffect(() => {
    if (visibleProviders.length === 0) return;
    if (!visibleProviders.some((provider) => provider.id === activeProviderId)) {
      setActiveProviderId(
        visibleProviders.find((provider) => provider.id === "openai")?.id ??
          visibleProviders[0].id,
      );
    }
  }, [activeProviderId, visibleProviders]);

  useEffect(() => {
    void api
      .loadSettings()
      .then((settings) => {
        const nextFontSize = clampInteger(
          Number(settings.overlayFontSize ?? 18),
          OVERLAY_FONT_SIZE_MIN,
          OVERLAY_FONT_SIZE_MAX,
        );
        const nextOpacity = clampInteger(
          Number(settings.overlayOpacity ?? 94),
          OVERLAY_OPACITY_MIN,
          OVERLAY_OPACITY_MAX,
        );
        setOverlayFontSize(nextFontSize);
        setOverlayFontSizeInput(String(nextFontSize));
        setOverlayOpacity(nextOpacity);
        setOverlayOpacityInput(String(nextOpacity));
      })
      .catch((error) => onError(String(error)));
  }, [onError]);

  useEffect(() => {
    setApiKey("");
    setApiKeyVisible(false);
    setBaseUrl(configuredString(openAiStatus, "baseUrl", baseUrlDefault));
    setTranscriptionModel(
      configuredString(
        openAiStatus,
        "transcriptionModel",
        transcriptionModelDefault,
      ),
    );
    setAnalysisModel(
      configuredString(openAiStatus, "analysisModel", analysisModelDefault),
    );
  }, [
    openAiStatus?.stored,
    openAiStatus?.configured,
    openAiStatus?.updatedAt,
    baseUrlDefault,
    transcriptionModelDefault,
    analysisModelDefault,
  ]);

  useEffect(() => {
    setMockScenario(
      configuredString(mockStatus, "scenario", mockScenarioDefault),
    );
  }, [mockStatus?.configured, mockStatus?.updatedAt, mockScenarioDefault]);

  useEffect(() => {
    if (activeCategory !== "about" || mediaRuntime !== null || mediaRuntimeLoading) {
      return;
    }
    setMediaRuntimeLoading(true);
    void api
      .mediaRuntimeStatus()
      .then(setMediaRuntime)
      .catch(() =>
        setMediaRuntime({
          available: false,
          bundled: true,
          mode: "unavailable",
          target: "unknown",
          expectedVersion: "8.1.2",
          ffmpeg: {
            available: false,
            integrityVerified: false,
            expectedSha256: "",
            errorCode: "ERR_MEDIA_RUNTIME_UNAVAILABLE",
          },
          ffprobe: {
            available: false,
            integrityVerified: false,
            expectedSha256: "",
            errorCode: "ERR_MEDIA_RUNTIME_UNAVAILABLE",
          },
        }),
      )
      .finally(() => setMediaRuntimeLoading(false));
  }, [activeCategory, mediaRuntime, mediaRuntimeLoading]);

  async function persistPreference(
    key: string,
    value: unknown,
    announce = true,
  ) {
    try {
      await api.saveSetting(key, value);
      onError(null);
      if (announce) showFeedback(t("settings.savedFeedback"));
    } catch (error) {
      showFeedback(t("settings.saveFailedFeedback"), "error");
      onError(String(error));
    }
  }

  function updatePreferences(next: LanguagePreferences) {
    onPreferences(next);
    void persistPreference("languagePreferences", next);
  }

  function updateOverlayFontSize(value: number, announce = false) {
    const next = clampInteger(
      value,
      OVERLAY_FONT_SIZE_MIN,
      OVERLAY_FONT_SIZE_MAX,
    );
    setOverlayFontSize(next);
    setOverlayFontSizeInput(String(next));
    void persistPreference("overlayFontSize", next, announce);
  }

  function updateOverlayOpacity(value: number, announce = false) {
    const next = clampInteger(value, OVERLAY_OPACITY_MIN, OVERLAY_OPACITY_MAX);
    setOverlayOpacity(next);
    setOverlayOpacityInput(String(next));
    void persistPreference("overlayOpacity", next, announce);
  }

  function commitOverlayFontSizeInput(announce = true) {
    updateOverlayFontSize(
      parseIntegerDraft(
        overlayFontSizeInput,
        overlayFontSize,
        OVERLAY_FONT_SIZE_MIN,
        OVERLAY_FONT_SIZE_MAX,
      ),
      announce,
    );
  }

  function commitOverlayOpacityInput(announce = true) {
    updateOverlayOpacity(
      parseIntegerDraft(
        overlayOpacityInput,
        overlayOpacity,
        OVERLAY_OPACITY_MIN,
        OVERLAY_OPACITY_MAX,
      ),
      announce,
    );
  }

  function handleOverlayNumberKeyDown(
    event: KeyboardEvent<HTMLInputElement>,
    commit: () => void,
    reset: () => void,
  ) {
    if (event.key === "Enter") {
      event.preventDefault();
      commit();
    }
    if (event.key === "Escape") {
      event.preventDefault();
      reset();
    }
  }

  async function saveOpenAi(event: FormEvent) {
    event.preventDefault();
    if (providerBusy) return;
    setProviderBusy(true);
    dismissFeedback();
    try {
      await api.saveProviderCredentials("openai", {
        apiKey,
        baseUrl: baseUrlDefault,
        transcriptionModel: transcriptionModelDefault,
        analysisModel: analysisModelDefault,
      });
      setApiKey("");
      await onRefreshStatuses();
      onError(null);
      showFeedback(t("providers.credentialsSaved"));
    } catch (error) {
      setApiKey("");
      showFeedback(t("providers.credentialsSaveFailed"), "error");
      onError(String(error));
    } finally {
      setProviderBusy(false);
    }
  }

  async function setDefaultProvider(providerId: string) {
    if (!canUseAsDefaultProvider(providerId)) return;
    if (!providerIsReady(providerId, providerStatuses)) return;
    onDefaultProvider(providerId);
    await persistPreference("defaultProviderId", providerId);
  }

  async function fallBackFromOpenAiDefault() {
    if (defaultProviderId !== "openai") return;
    const fallback = ENABLE_DEV_TOOLS ? "mock" : "openai";
    onDefaultProvider(fallback);
    await persistPreference("defaultProviderId", fallback, false);
  }

  async function removeOpenAiSecret() {
    if (providerBusy) return;
    setProviderBusy(true);
    try {
      await api.removeProviderSecret("openai", "apiKey");
      await onRefreshStatuses();
      await fallBackFromOpenAiDefault();
      setRemoveApiKeyOpen(false);
      setApiKey("");
      setApiKeyVisible(false);
      showFeedback(t("providers.apiKeyRemoved"));
      onError(null);
    } catch (error) {
      showFeedback(t("providers.apiKeyRemoveFailed"), "error");
      onError(String(error));
    } finally {
      setProviderBusy(false);
    }
  }

  async function removeOpenAi() {
    if (providerBusy) return;
    setProviderBusy(true);
    try {
      await api.removeProviderCredentials("openai");
      await onRefreshStatuses();
      await fallBackFromOpenAiDefault();
      setRemoveProviderOpen(false);
      setApiKey("");
      setApiKeyVisible(false);
      showFeedback(t("providers.configurationRemoved"));
      onError(null);
    } catch (error) {
      showFeedback(t("providers.configurationRemoveFailed"), "error");
      onError(String(error));
    } finally {
      setProviderBusy(false);
    }
  }

  async function saveMock() {
    if (mockBusy) return;
    setMockBusy(true);
    try {
      await api.saveProviderCredentials("mock", { scenario: mockScenario });
      await onRefreshStatuses();
      showFeedback(t("settings.mockSavedFeedback"));
      onError(null);
    } catch (error) {
      showFeedback(t("settings.mockSaveFailedFeedback"), "error");
      onError(String(error));
    } finally {
      setMockBusy(false);
    }
  }

  return (
    <section className="settingsPage">
      <div className="pageHeader settingsHeader">
        <div>
          <span className="pageEyebrow">{t("settings.workspaceEyebrow")}</span>
          <h1>{t("settings.workspaceTitle")}</h1>
          <p>{t("settings.workspaceDescription")}</p>
        </div>
      </div>

      {feedback && (
        <div
          className={`settingsActionToast settingsActionToast-${feedback.tone}`}
          role={feedback.tone === "error" ? "alert" : "status"}
          aria-live={feedback.tone === "error" ? "assertive" : "polite"}
          aria-atomic="true"
        >
          <span className="settingsActionToastMark" aria-hidden="true">
            {feedback.tone === "error" ? "!" : "✓"}
          </span>
          <span>{feedback.message}</span>
          <button
            className="settingsActionToastDismiss"
            type="button"
            aria-label={t("common.dismiss")}
            onClick={dismissFeedback}
          >
            <Icon name="close" size={14} />
          </button>
        </div>
      )}

      <div className="settingsWorkspace">
        <nav
          className="settingsCategoryNav"
          aria-label={t("settings.categoryNavigation")}
        >
          {settingsCategories.map((category) => (
            <button
              type="button"
              key={category.id}
              className={activeCategory === category.id ? "active" : ""}
              aria-current={activeCategory === category.id ? "page" : undefined}
              onClick={() => {
                setActiveCategory(category.id);
                dismissFeedback();
              }}
            >
              <Icon name={category.icon} size={17} />
              <span>{t(`settings.categories.${category.id}`)}</span>
            </button>
          ))}
        </nav>

        <div className="settingsContent">
          {activeCategory === "general" && (
            <SettingsSection
              title={t("settings.generalTitle")}
              description={t("settings.generalDescription")}
            >
              <SettingRow
                label={t("settings.defaultProvider")}
                description={t("settings.defaultProviderHint")}
              >
                <select
                  value={defaultProviderId}
                  onChange={(event) =>
                    void setDefaultProvider(event.target.value)
                  }
                >
                  {defaultProviderOptions.map((provider) => {
                    const ready = providerIsReady(
                      provider.id,
                      providerStatuses,
                    );
                    const suffix = isUiOnlyProvider(provider.id)
                      ? t("providers.uiExtensionTestOnly")
                      : !ready
                        ? t("providers.notReady")
                        : "";
                    return (
                      <option
                        value={provider.id}
                        key={provider.id}
                        disabled={!ready}
                      >
                        {t(provider.displayNameKey)}
                        {suffix ? ` — ${suffix}` : ""}
                      </option>
                    );
                  })}
                </select>
              </SettingRow>
              <SettingRow
                label={t("settings.inactivityLock")}
                description={t("settings.inactivityHint")}
              >
                <select
                  value={inactivityMinutes === null ? "never" : String(inactivityMinutes)}
                  onChange={(event) => {
                    const value =
                      event.target.value === "never"
                        ? null
                        : (Number(
                            event.target.value,
                          ) as InactivityLockMinutes);
                    onInactivityMinutes(value);
                    void persistPreference("inactivityLockMinutes", value);
                  }}
                >
                  <option value="never">{t("settings.inactivityNever")}</option>
                  {[5, 15, 30, 60].map((minutes) => (
                    <option value={minutes} key={minutes}>
                      {t("settings.inactivityMinutes", { minutes })}
                    </option>
                  ))}
                </select>
              </SettingRow>
              <div className="settingsInfoCard">
                <Icon name="lock" size={18} />
                <div>
                  <strong>{t("settings.manualLockTitle")}</strong>
                  <p>{t("settings.manualLockDescription")}</p>
                </div>
              </div>
            </SettingsSection>
          )}

          {activeCategory === "appearance" && (
            <SettingsSection
              title={t("settings.appearanceTitle")}
              description={t("settings.appearanceDescription")}
            >
              <SettingRow
                label={t("settings.overlayFontSize")}
                description={t("settings.overlayFontSizeHint", {
                  value: overlayFontSize,
                })}
              >
                <div className="rangeNumberControl">
                  <input
                    type="range"
                    min={OVERLAY_FONT_SIZE_MIN}
                    max={OVERLAY_FONT_SIZE_MAX}
                    value={overlayFontSize}
                    onChange={(event) =>
                      updateOverlayFontSize(Number(event.target.value), false)
                    }
                    onPointerUp={() => showFeedback(t("settings.savedFeedback"))}
                    onKeyUp={() => showFeedback(t("settings.savedFeedback"))}
                    aria-label={t("settings.overlayFontSize")}
                  />
                  <label className="numberUnitInput">
                    <input
                      type="number"
                      min={OVERLAY_FONT_SIZE_MIN}
                      max={OVERLAY_FONT_SIZE_MAX}
                      step="1"
                      value={overlayFontSizeInput}
                      onChange={(event) =>
                        setOverlayFontSizeInput(event.target.value)
                      }
                      onBlur={() => commitOverlayFontSizeInput(true)}
                      onKeyDown={(event) =>
                        handleOverlayNumberKeyDown(
                          event,
                          () => commitOverlayFontSizeInput(true),
                          () => setOverlayFontSizeInput(String(overlayFontSize)),
                        )
                      }
                      aria-label={t("settings.overlayFontSize")}
                    />
                    <span>{t("settings.units.px")}</span>
                  </label>
                </div>
              </SettingRow>
              <SettingRow
                label={t("settings.overlayTransparency")}
                description={t("settings.overlayOpacityHint", {
                  value: overlayOpacity,
                })}
              >
                <div className="rangeNumberControl">
                  <input
                    type="range"
                    min={OVERLAY_OPACITY_MIN}
                    max={OVERLAY_OPACITY_MAX}
                    value={overlayOpacity}
                    onChange={(event) =>
                      updateOverlayOpacity(Number(event.target.value), false)
                    }
                    onPointerUp={() => showFeedback(t("settings.savedFeedback"))}
                    onKeyUp={() => showFeedback(t("settings.savedFeedback"))}
                    aria-label={t("settings.overlayTransparency")}
                  />
                  <label className="numberUnitInput">
                    <input
                      type="number"
                      min={OVERLAY_OPACITY_MIN}
                      max={OVERLAY_OPACITY_MAX}
                      step="1"
                      value={overlayOpacityInput}
                      onChange={(event) =>
                        setOverlayOpacityInput(event.target.value)
                      }
                      onBlur={() => commitOverlayOpacityInput(true)}
                      onKeyDown={(event) =>
                        handleOverlayNumberKeyDown(
                          event,
                          () => commitOverlayOpacityInput(true),
                          () => setOverlayOpacityInput(String(overlayOpacity)),
                        )
                      }
                      aria-label={t("settings.overlayTransparency")}
                    />
                    <span>{t("settings.units.percent")}</span>
                  </label>
                </div>
              </SettingRow>
            </SettingsSection>
          )}

          {activeCategory === "audio" && (
            <SettingsSection
              title={t("settings.audioTitle")}
              description={t("settings.audioDescription")}
            >
              <div className="settingsInfoCard settingsInfoCard-wide">
                <Icon name="inPerson" size={19} />
                <div>
                  <strong>{t("settings.audioDeviceTitle")}</strong>
                  <p>{t("settings.audioDeviceDescription")}</p>
                </div>
              </div>
              <div className="settingsInfoCard settingsInfoCard-wide">
                <Icon name="shield" size={19} />
                <div>
                  <strong>{t("settings.audioPrivacyTitle")}</strong>
                  <p>{t("settings.audioPrivacyDescription")}</p>
                </div>
              </div>
            </SettingsSection>
          )}

          {activeCategory === "language" && (
            <SettingsSection
              title={t("settings.languageTitle")}
              description={t("settings.languageDescription")}
            >
              <div className="settingsFormGrid">
                <SettingField label={t("settings.sourceLanguageMode")}>
                  <select
                    value={preferences.sourceLanguageMode}
                    onChange={(event) => {
                      updatePreferences({
                        ...preferences,
                        sourceLanguageMode: event.target.value as
                          | "auto"
                          | "specified",
                        sourceLanguage: preferences.sourceLanguage ?? "en",
                      });
                    }}
                  >
                    <option value="auto">{t("realtime.autoDetect")}</option>
                    <option value="specified">{t("realtime.specified")}</option>
                  </select>
                  {preferences.sourceLanguageMode === "auto" && (
                    <span className="fieldHint">
                      {t("settings.sourceAutoHint")}
                    </span>
                  )}
                </SettingField>

                {preferences.sourceLanguageMode === "specified" && (
                  <SettingField label={t("settings.sourceLanguage")}>
                    <select
                      value={preferences.sourceLanguage ?? "en"}
                      onChange={(event) => {
                        updatePreferences({
                          ...preferences,
                          sourceLanguage: event.target.value,
                        });
                      }}
                    >
                      {languageOptions()}
                    </select>
                  </SettingField>
                )}

                <SettingField label={t("settings.translationTarget")}>
                  <select
                    value={preferences.translationTargetLanguage}
                    onChange={(event) => {
                      updatePreferences({
                        ...preferences,
                        translationTargetLanguage: event.target.value,
                      });
                    }}
                  >
                    <option value={NO_TRANSLATION}>
                      {t("settings.noTranslation")}
                    </option>
                    {languageOptions()}
                  </select>
                  {preferences.translationTargetLanguage === NO_TRANSLATION && (
                    <span className="fieldHint">
                      {t("settings.translationDisabledHint")}
                    </span>
                  )}
                </SettingField>

                <SettingField label={t("settings.analysisLanguage")}>
                  <select
                    value={preferences.analysisOutputLanguage}
                    onChange={(event) => {
                      updatePreferences({
                        ...preferences,
                        analysisOutputLanguage: event.target.value,
                      });
                    }}
                  >
                    <option value={SAME_AS_TRANSLATION}>
                      {t("settings.sameAsTranslation")}
                    </option>
                    {languageOptions()}
                  </select>
                  {preferences.analysisOutputLanguage === SAME_AS_TRANSLATION &&
                    preferences.translationTargetLanguage === NO_TRANSLATION && (
                      <span className="fieldHint">
                        {t("settings.analysisFallbackHint")}
                      </span>
                    )}
                  <span className="effectiveSetting">
                    {t("settings.resolvedOutput", {
                      language: t(
                        languageLabelKey(resolved.analysisOutputLanguage),
                      ),
                    })}
                  </span>
                </SettingField>

                <SettingField label={t("settings.minutesTarget")}>
                  <select
                    value={preferences.minutesOutputLanguage}
                    onChange={(event) => {
                      updatePreferences({
                        ...preferences,
                        minutesOutputLanguage: event.target.value,
                      });
                    }}
                  >
                    <option value={SAME_AS_ANALYSIS}>
                      {t("settings.sameAsAnalysis")}
                    </option>
                    {languageOptions()}
                  </select>
                  <span className="effectiveSetting">
                    {t("settings.resolvedOutput", {
                      language: t(
                        languageLabelKey(resolved.minutesOutputLanguage),
                      ),
                    })}
                  </span>
                </SettingField>
              </div>
            </SettingsSection>
          )}

          {activeCategory === "provider" && (
            <SettingsSection
              title={t("providers.title")}
              description={t("providers.realApiOptional")}
            >
              <SettingRow
                label={t("providers.activeProvider")}
                description={t("providers.activeProviderDescription")}
              >
                <select
                  value={activeProvider?.id ?? ""}
                  onChange={(event) => setActiveProviderId(event.target.value)}
                >
                  {visibleProviders.map((provider) => (
                    <option value={provider.id} key={provider.id}>
                      {t(provider.displayNameKey)}
                    </option>
                  ))}
                </select>
              </SettingRow>

              <div className="providerOverviewGrid">
                {visibleProviders.map((provider) => {
                  const status = providerStatuses.find(
                    (value) => value.providerId === provider.id,
                  );
                  const isDefault = defaultProviderId === provider.id;
                  const canBeDefault = canUseAsDefaultProvider(provider.id);
                  const canSetDefault =
                    canBeDefault && (status?.configured || isUiOnlyProvider(provider.id));
                  return (
                    <article className="providerOverviewCard" key={provider.id}>
                      <div className="providerOverviewHeader">
                        <span className="providerIdentity">
                          <Icon name="analyze" size={18} />
                          <strong>{t(provider.displayNameKey)}</strong>
                        </span>
                        <span
                          className={providerStatusClass(provider.id, status)}
                        >
                          {providerStatusLabel(provider.id, status)}
                        </span>
                      </div>
                      <p>{t(providerDescriptionKey(provider.id))}</p>
                      <div className="providerOverviewActions">
                        {isDefault ? (
                          <span className="defaultProviderBadge">
                            {t("providers.defaultProvider")}
                          </span>
                        ) : (
                          <button
                            type="button"
                            className="secondaryButton"
                            disabled={!canSetDefault}
                            onClick={() => void setDefaultProvider(provider.id)}
                          >
                            {t("providers.setAsDefault")}
                          </button>
                        )}
                      </div>
                    </article>
                  );
                })}
              </div>

              {activeProvider && activeProviderPanelKind === "openaiApiKey" && (
                <form className="providerSettingsCard" onSubmit={saveOpenAi}>
                  <div className="providerCardHeader">
                    <div>
                      <span className="providerIdentity">
                        <Icon name="analyze" size={18} />
                        <strong>{t(activeProvider.displayNameKey)}</strong>
                      </span>
                      <p>{t(providerDescriptionKey(activeProvider.id))}</p>
                    </div>
                    <span
                      className={providerStatusClass(activeProvider.id, activeProviderStatus)}
                    >
                      {providerStatusLabel(activeProvider.id, activeProviderStatus)}
                    </span>
                  </div>
                  {api.isNative ? (
                    <>
                      <section className="providerCredentialPanel">
                        <div className="providerCredentialHeader">
                          <div>
                            <h3>{t("providers.apiKeyTitle")}</h3>
                            <p>{t("providers.apiKeySecurityDescription")}</p>
                          </div>
                          <span
                            className={openAiKeySaved ? "statusGood" : "statusMuted"}
                          >
                            {openAiKeySaved
                              ? t("providers.apiKeySaved")
                              : t("providers.apiKeyMissing")}
                          </span>
                        </div>
                        {openAiKeySaved && (
                          <div className="savedSecretSummary" aria-label={t("providers.apiKeySaved")}>
                            <span className="savedSecretMask" aria-hidden="true">
                              ••••••••••••
                            </span>
                            <span>{t("providers.savedSecretNeverShown")}</span>
                          </div>
                        )}
                        <SettingField
                          label={
                            openAiKeySaved
                              ? t("providers.replaceApiKey")
                              : t("providers.fields.apiKey")
                          }
                        >
                          <div className="secretInputRow">
                            <input
                              type={apiKeyVisible ? "text" : "password"}
                              autoComplete="new-password"
                              spellCheck={false}
                              value={apiKey}
                              placeholder={
                                openAiKeySaved
                                  ? t("providers.apiKeyRetained")
                                  : t("providers.apiKeyPlaceholder")
                              }
                              onChange={(event) => setApiKey(event.target.value)}
                            />
                            <button
                              type="button"
                              className="secondaryButton secretVisibilityButton"
                              disabled={!apiKey}
                              aria-pressed={apiKeyVisible}
                              onClick={() => setApiKeyVisible((value) => !value)}
                            >
                              {apiKeyVisible
                                ? t("providers.hideEnteredKey")
                                : t("providers.showEnteredKey")}
                            </button>
                          </div>
                          <span className="fieldHint">
                            {openAiKeySaved
                              ? t("providers.apiKeyReplaceHint")
                              : t("providers.apiKeyAddHint")}
                          </span>
                        </SettingField>
                        {openAiKeySaved && (
                          <button
                            type="button"
                            className="dangerLinkButton"
                            disabled={providerBusy}
                            onClick={() => setRemoveApiKeyOpen(true)}
                          >
                            {t("providers.deleteApiKey")}
                          </button>
                        )}
                      </section>

                      <div className="providerManagedConfiguration">
                        <div className="settingsInfoCard settingsInfoCard-wide">
                          <Icon name="shield" size={18} />
                          <div>
                            <strong>{t("providers.officialEndpointTitle")}</strong>
                            <p>{t("providers.officialEndpointDescription")}</p>
                          </div>
                        </div>
                        {hasLegacyCustomOpenAiEndpoint && (
                          <div className="notice">
                            {t("providers.legacyCustomEndpointNotice")}
                          </div>
                        )}
                        <div className="settingsFormActions">
                          <button
                            className="primaryButton"
                            disabled={providerBusy || (!openAiKeySaved && !apiKey.trim())}
                          >
                            {providerBusy
                              ? t("common.saving")
                              : openAiKeySaved && apiKey.trim()
                                ? t("providers.replaceKeyAndSave")
                                : t("providers.saveProvider")}
                          </button>
                          {openAiStatus?.stored && (
                            <button
                              type="button"
                              className="dangerButton"
                              disabled={providerBusy}
                              onClick={() => setRemoveProviderOpen(true)}
                            >
                              {t("providers.removeConfiguration")}
                            </button>
                          )}
                        </div>
                      </div>

                      <ProviderModelAssignments
                        provider={activeProvider}
                        status={activeProviderStatus}
                      />
                    </>
                  ) : (
                    <div className="notice">
                      {t("providers.desktopOnlyCredentials")}
                    </div>
                  )}
                </form>
              )}

              {activeProvider && activeProviderPanelKind !== "openaiApiKey" && (
                <article className="providerSettingsCard">
                  <div className="providerCardHeader">
                    <div>
                      <span className="providerIdentity">
                        <Icon name="analyze" size={18} />
                        <strong>{t(activeProvider.displayNameKey)}</strong>
                      </span>
                      <p>{t(providerDescriptionKey(activeProvider.id))}</p>
                    </div>
                    <span
                      className={providerStatusClass(activeProvider.id, activeProviderStatus)}
                    >
                      {providerStatusLabel(activeProvider.id, activeProviderStatus)}
                    </span>
                  </div>
                  {activeProviderPanelKind === "developerDiagnostics" && (
                    <div className="notice">
                      {t("providers.mockDeveloperOnlyNotice")}
                    </div>
                  )}
                  {activeProviderPanelKind === "uiExtensionTest" && (
                    <div className="notice">
                      {t("providers.testAdapter.uiOnlyNotice")}
                    </div>
                  )}
                  <ProviderModelAssignments
                    provider={activeProvider}
                    status={activeProviderStatus}
                  />
                </article>
              )}

              {activeProvider && (
                <ProviderCapabilitySection provider={activeProvider} />
              )}
            </SettingsSection>
          )}

          {activeCategory === "security" && (
            <SettingsSection
              title={t("settings.securityTitle")}
              description={t("settings.securityDescription")}
            >
              <div className="settingsInfoCard settingsInfoCard-wide">
                <Icon name="shield" size={19} />
                <div>
                  <strong>{t("settings.privacyTitle")}</strong>
                  <p>{t("settings.privacyBody")}</p>
                </div>
              </div>
              <section className="dangerZone">
                <div>
                  <h3>{t("settings.dangerZoneTitle")}</h3>
                  <p>{t("settings.dangerZoneDescription")}</p>
                </div>
                <button
                  type="button"
                  className="dangerButton"
                  onClick={() => setResetOpen(true)}
                >
                  <Icon name="delete" size={16} />
                  {t("unlock.resetVault")}
                </button>
              </section>
            </SettingsSection>
          )}

          {ENABLE_DEV_TOOLS && activeCategory === "advanced" && (
            <SettingsSection
              title={t("settings.advancedTitle")}
              description={t("settings.advancedDescription")}
            >
              <div className="providerSettingsCard compactProviderCard">
                <div className="providerCardHeader">
                  <div>
                    <span className="providerIdentity">
                      <Icon name="analyze" size={18} />
                      <strong>{t("providers.mock.displayName")}</strong>
                    </span>
                    <p>{t("settings.mockDescription")}</p>
                  </div>
                  <span className="statusGood">
                    {t("providers.available")}
                  </span>
                </div>
                <SettingRow
                  label={t("providers.fields.mockScenario")}
                  description={t("settings.mockScenarioHint")}
                >
                  <div className="settingActionControl">
                    <select
                      value={mockScenario}
                      onChange={(event) => setMockScenario(event.target.value)}
                    >
                      {[
                        "normal",
                        "timeout",
                        "quota",
                        "authentication",
                        "unavailable",
                        "unsupported",
                      ].map((value) => (
                        <option key={value} value={value}>
                          {t(`providers.mockScenarios.${value}`)}
                        </option>
                      ))}
                    </select>
                    <button
                      type="button"
                      className="primaryButton"
                      disabled={mockBusy}
                      onClick={() => void saveMock()}
                    >
                      {mockBusy ? t("common.saving") : t("common.save")}
                    </button>
                  </div>
                </SettingRow>
              </div>
            </SettingsSection>
          )}

          {activeCategory === "about" && (
            <SettingsSection
              title={t("settings.aboutTitle")}
              description={t("settings.aboutDescription")}
            >
              <div className="aboutProductCard">
                <span className="aboutBrandMark">
                  <Icon name="brand" size={28} />
                </span>
                <div>
                  <h3>{t("common.appName")}</h3>
                  <p>{t("common.tagline")}</p>
                  <span>{t("settings.version", { version: APP_VERSION })}</span>
                </div>
              </div>
              <div className="settingsInfoCard settingsInfoCard-wide">
                <Icon name="library" size={19} />
                <div>
                  <strong>{t("settings.localFirstTitle")}</strong>
                  <p>{t("settings.localFirstDescription")}</p>
                </div>
              </div>
              <div className="settingsInfoCard settingsInfoCard-wide">
                <Icon name="media" size={19} />
                <div>
                  <strong>{t("settings.mediaRuntimeTitle")}</strong>
                  <p>
                    {mediaRuntimeLoading || mediaRuntime === null
                      ? t("settings.mediaRuntimeChecking")
                      : mediaRuntime.available
                        ? t("settings.mediaRuntimeReady", {
                            version: mediaRuntime.expectedVersion,
                            target: mediaRuntime.target,
                          })
                        : t("settings.mediaRuntimeUnavailable", {
                            version: mediaRuntime.expectedVersion,
                          })}
                  </p>
                </div>
              </div>
            </SettingsSection>
          )}
        </div>
      </div>

      <Dialog
        open={removeApiKeyOpen}
        title={t("providers.deleteApiKeyDialogTitle")}
        description={t("providers.deleteApiKeyDialogDescription")}
        tone="danger"
        closeLabel={t("common.close")}
        onClose={() => setRemoveApiKeyOpen(false)}
        actions={
          <>
            <button
              type="button"
              disabled={providerBusy}
              onClick={() => setRemoveApiKeyOpen(false)}
            >
              {t("common.cancel")}
            </button>
            <button
              type="button"
              className="dangerButton"
              disabled={providerBusy}
              onClick={() => void removeOpenAiSecret()}
            >
              {providerBusy
                ? t("common.processing")
                : t("providers.deleteApiKey")}
            </button>
          </>
        }
      />

      <Dialog
        open={removeProviderOpen}
        title={t("providers.removeDialogTitle")}
        description={t("providers.removeDialogDescription")}
        tone="danger"
        closeLabel={t("common.close")}
        onClose={() => setRemoveProviderOpen(false)}
        actions={
          <>
            <button
              type="button"
              disabled={providerBusy}
              onClick={() => setRemoveProviderOpen(false)}
            >
              {t("common.cancel")}
            </button>
            <button
              type="button"
              className="dangerButton"
              disabled={providerBusy}
              onClick={() => void removeOpenAi()}
            >
              {providerBusy
                ? t("common.processing")
                : t("providers.removeConfiguration")}
            </button>
          </>
        }
      />

      <ResetVaultDialog
        open={resetOpen}
        onClose={() => setResetOpen(false)}
        onError={(code) => {
          showFeedback(t("settings.resetFailedFeedback"), "error");
          onError(code);
        }}
      />
    </section>
  );
}

function SettingsSection({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <section className="settingsSection">
      <header className="settingsSectionHeader">
        <h2>{title}</h2>
        <p>{description}</p>
      </header>
      <div className="settingsSectionBody">{children}</div>
    </section>
  );
}

function ProviderModelAssignments({
  provider,
  status,
}: {
  provider: ProviderDefinition;
  status: ProviderConfigurationStatus | undefined;
}) {
  if (provider.modelAssignments.length === 0) return null;
  return (
    <section className="modelAssignmentSection">
      <div className="settingsSubheader">
        <h3>{t("providers.modelAssignments")}</h3>
        <p>{t("providers.modelAssignmentsDescription")}</p>
      </div>
      <div className="modelAssignmentGrid">
        {provider.modelAssignments.map((assignment) => (
          <div className="modelAssignmentRow" key={assignment.capability}>
            <span>{t(`providers.capabilityLabels.${assignment.capability}`)}</span>
            <strong>
              {configuredModel(
                provider,
                status,
                assignment.configurationFieldId,
              )}
            </strong>
          </div>
        ))}
      </div>
    </section>
  );
}

function ProviderCapabilitySection({ provider }: { provider: ProviderDefinition }) {
  return (
    <section className="providerCapabilitySection">
      <div className="settingsSubheader">
        <h3>{t("providers.capabilities")}</h3>
        <p>{t("providers.capabilitiesDescription")}</p>
      </div>
      <details open>
        <summary>{t(provider.displayNameKey)}</summary>
        <div className="capGrid">
          {Object.entries(provider.capabilities)
            .filter(([, value]) => typeof value === "boolean")
            .map(([key, value]) => (
              <span className={value ? "capabilityAvailable" : ""} key={key}>
                {t(`providers.capabilityLabels.${key}`)}: {value
                  ? t("providers.available")
                  : t("providers.unavailable")}
              </span>
            ))}
        </div>
      </details>
    </section>
  );
}

function SettingRow({
  label,
  description,
  children,
}: {
  label: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <div className="settingRow">
      <div className="settingRowCopy">
        <strong>{label}</strong>
        <p>{description}</p>
      </div>
      <div className="settingRowControl">{children}</div>
    </div>
  );
}

function SettingField({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <label className="settingField">
      <span className="settingFieldLabel">{label}</span>
      {children}
    </label>
  );
}
