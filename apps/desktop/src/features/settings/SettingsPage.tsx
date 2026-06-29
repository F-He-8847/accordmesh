import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Dialog } from "../../components/Dialog";
import { ResetVaultDialog } from "../../components/ResetVaultDialog";
import { Icon } from "../../components/Icon";
import type { IconName } from "../../components/Icon";
import { t } from "../../i18n";
import { api } from "../../shared/api";
import { APP_VERSION } from "../../shared/appVersion";
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
  { id: "advanced", icon: "sort" },
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
  if (providerId === "openai" && status?.maskedSummary === "ready") {
    return t("providers.configuredShort");
  }
  return t(`providers.status.${status?.maskedSummary ?? "not_configured"}`);
}

function providerIsReady(
  providerId: string,
  statuses: ProviderConfigurationStatus[],
) {
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
  const [apiKey, setApiKey] = useState("");
  const [apiKeyVisible, setApiKeyVisible] = useState(false);
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com/v1");
  const [transcriptionModel, setTranscriptionModel] = useState(
    "gpt-4o-mini-transcribe",
  );
  const [analysisModel, setAnalysisModel] = useState("gpt-5-mini");
  const [mockScenario, setMockScenario] = useState("normal");
  const [overlayFontSize, setOverlayFontSize] = useState(18);
  const [overlayOpacity, setOverlayOpacity] = useState(94);
  const [feedback, setFeedback] = useState<SettingsFeedback | null>(null);
  const feedbackTimer = useRef<number | undefined>(undefined);
  const [providerBusy, setProviderBusy] = useState(false);
  const [mockBusy, setMockBusy] = useState(false);
  const [removeProviderOpen, setRemoveProviderOpen] = useState(false);
  const [removeApiKeyOpen, setRemoveApiKeyOpen] = useState(false);
  const [resetOpen, setResetOpen] = useState(false);

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
    void api
      .loadSettings()
      .then((settings) => {
        setOverlayFontSize(Number(settings.overlayFontSize ?? 18));
        setOverlayOpacity(Number(settings.overlayOpacity ?? 94));
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

  async function saveOpenAi(event: FormEvent) {
    event.preventDefault();
    if (providerBusy) return;
    setProviderBusy(true);
    dismissFeedback();
    try {
      await api.saveProviderCredentials("openai", {
        apiKey,
        baseUrl,
        transcriptionModel,
        analysisModel,
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
    if (!providerIsReady(providerId, providerStatuses)) return;
    onDefaultProvider(providerId);
    await persistPreference("defaultProviderId", providerId);
  }

  async function fallBackFromOpenAiDefault() {
    if (defaultProviderId !== "openai") return;
    onDefaultProvider("mock");
    await persistPreference("defaultProviderId", "mock", false);
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
                  {providers.map((provider) => {
                    const ready = providerIsReady(
                      provider.id,
                      providerStatuses,
                    );
                    return (
                      <option
                        value={provider.id}
                        key={provider.id}
                        disabled={!ready}
                      >
                        {t(provider.displayNameKey)}
                        {!ready ? ` — ${t("providers.notReady")}` : ""}
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
                <input
                  type="range"
                  min="14"
                  max="28"
                  value={overlayFontSize}
                  onChange={(event) => {
                    const value = Number(event.target.value);
                    setOverlayFontSize(value);
                    void persistPreference("overlayFontSize", value, false);
                  }}
                  onPointerUp={() => showFeedback(t("settings.savedFeedback"))}
                  onKeyUp={() => showFeedback(t("settings.savedFeedback"))}
                />
              </SettingRow>
              <SettingRow
                label={t("settings.overlayTransparency")}
                description={t("settings.overlayOpacityHint", {
                  value: overlayOpacity,
                })}
              >
                <input
                  type="range"
                  min="60"
                  max="100"
                  value={overlayOpacity}
                  onChange={(event) => {
                    const value = Number(event.target.value);
                    setOverlayOpacity(value);
                    void persistPreference("overlayOpacity", value, false);
                  }}
                  onPointerUp={() => showFeedback(t("settings.savedFeedback"))}
                  onKeyUp={() => showFeedback(t("settings.savedFeedback"))}
                />
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
              <div className="providerOverviewGrid">
                {providers.map((provider) => {
                  const status = providerStatuses.find(
                    (value) => value.providerId === provider.id,
                  );
                  const isDefault = defaultProviderId === provider.id;
                  return (
                    <article className="providerOverviewCard" key={provider.id}>
                      <div className="providerOverviewHeader">
                        <span className="providerIdentity">
                          <Icon name="analyze" size={18} />
                          <strong>{t(provider.displayNameKey)}</strong>
                        </span>
                        <span
                          className={
                            status?.configured
                              ? "statusGood"
                              : status?.stored
                                ? "statusWarning"
                                : "statusMuted"
                          }
                        >
                          {providerStatusLabel(provider.id, status)}
                        </span>
                      </div>
                      <p>
                        {provider.id === "mock"
                          ? t("providers.mockDescription")
                          : t("providers.openAiDescription")}
                      </p>
                      <div className="providerOverviewActions">
                        {isDefault ? (
                          <span className="defaultProviderBadge">
                            {t("providers.defaultProvider")}
                          </span>
                        ) : (
                          <button
                            type="button"
                            className="secondaryButton"
                            disabled={!status?.configured}
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

              <form className="providerSettingsCard" onSubmit={saveOpenAi}>
                <div className="providerCardHeader">
                  <div>
                    <span className="providerIdentity">
                      <Icon name="analyze" size={18} />
                      <strong>{t("providers.openai.displayName")}</strong>
                    </span>
                    <p>{t("providers.openAiDescription")}</p>
                  </div>
                  <span
                    className={
                      openAiStatus?.configured
                        ? "statusGood"
                        : openAiStatus?.stored
                          ? "statusWarning"
                          : "statusMuted"
                    }
                  >
                    {providerStatusLabel("openai", openAiStatus)}
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

                    <div className="settingsFormGrid providerFormGrid">
                      <SettingField label={t("providers.fields.baseUrl")}>
                        <input
                          value={baseUrl}
                          onChange={(event) => setBaseUrl(event.target.value)}
                          required
                        />
                      </SettingField>
                      <SettingField
                        label={t("providers.fields.transcriptionModel")}
                      >
                        <input
                          value={transcriptionModel}
                          onChange={(event) =>
                            setTranscriptionModel(event.target.value)
                          }
                          required
                        />
                      </SettingField>
                      <SettingField label={t("providers.fields.analysisModel")}>
                        <input
                          value={analysisModel}
                          onChange={(event) =>
                            setAnalysisModel(event.target.value)
                          }
                          required
                        />
                      </SettingField>
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

                    {openAiDefinition && (
                      <section className="modelAssignmentSection">
                        <div className="settingsSubheader">
                          <h3>{t("providers.modelAssignments")}</h3>
                          <p>{t("providers.modelAssignmentsDescription")}</p>
                        </div>
                        <div className="modelAssignmentGrid">
                          {openAiDefinition.modelAssignments.map((assignment) => (
                            <div className="modelAssignmentRow" key={assignment.capability}>
                              <span>
                                {t(`providers.capabilityLabels.${assignment.capability}`)}
                              </span>
                              <strong>
                                {configuredModel(
                                  openAiDefinition,
                                  openAiStatus,
                                  assignment.configurationFieldId,
                                )}
                              </strong>
                            </div>
                          ))}
                        </div>
                      </section>
                    )}
                  </>
                ) : (
                  <div className="notice">
                    {t("providers.desktopOnlyCredentials")}
                  </div>
                )}
              </form>

              <section className="providerCapabilitySection">
                <div className="settingsSubheader">
                  <h3>{t("providers.capabilities")}</h3>
                  <p>{t("providers.capabilitiesDescription")}</p>
                </div>
                {providers.map((provider) => (
                  <details key={provider.id}>
                    <summary>{t(provider.displayNameKey)}</summary>
                    <div className="capGrid">
                      {Object.entries(provider.capabilities)
                        .filter(([, value]) => typeof value === "boolean")
                        .map(([key, value]) => (
                          <span
                            className={value ? "capabilityAvailable" : ""}
                            key={key}
                          >
                            {t(`providers.capabilityLabels.${key}`)}: {value
                              ? t("providers.available")
                              : t("providers.unavailable")}
                          </span>
                        ))}
                    </div>
                  </details>
                ))}
              </section>
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

          {activeCategory === "advanced" && (
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
