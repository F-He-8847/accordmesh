import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { LibraryPage } from "../features/library/LibraryPage";
import { MeetingStartPage } from "../features/online-meeting/MeetingStartPage";
import { ProjectDetailPage } from "../features/project-detail/ProjectDetailPage";
import { SettingsPage } from "../features/settings/SettingsPage";
import { UnlockPage } from "../features/unlock/UnlockPage";
import { UploadPage } from "../features/upload/UploadPage";
import { Icon, type IconName } from "../components/Icon";
import { api } from "../shared/api";
import {
  lockErrorForTrigger,
  shouldAutoLock,
  type LockTrigger,
} from "../shared/lockLifecycle";
import {
  DEFAULT_LANGUAGE_PREFERENCES,
  InactivityLockMinutes,
  normalizeInactivityLockMinutes,
  normalizeLanguagePreferences,
  resolveLanguagePreferences,
} from "../shared/languagePreferences";
import type {
  LanguagePreferences,
  MeetingProject,
  ProjectDetail,
  ProviderConfigurationStatus,
  ProviderDefinition,
  RealtimeMode,
  RealtimeStateUpdate,
} from "../shared/types";
import { t } from "../i18n";
import {
  releaseSafeProviderId,
  visibleProviderDefinitions,
} from "../shared/providerUiRegistry";

type Route =
  | { name: "library" }
  | { name: "online" }
  | { name: "inPerson" }
  | { name: "upload"; attachId?: string }
  | { name: "project"; projectId: string }
  | { name: "settings" };

function providerErrorCategory(code: string | null) {
  if (code === "ERR_TEST_PROVIDER_ADAPTER_UI_ONLY") return "configuration";
  if (!code?.startsWith("ERR_PROVIDER")) return null;
  if (["ERR_PROVIDER_AUTH", "ERR_PROVIDER_PERMISSION"].includes(code)) {
    return "authentication";
  }
  if ([
    "ERR_PROVIDER_NOT_CONFIGURED",
    "ERR_PROVIDER_CONFIG",
    "ERR_PROVIDER_OPENAI_BASE_URL",
    "ERR_PROVIDER_NOT_FOUND",
  ].includes(code)) return "configuration";
  if (["ERR_PROVIDER_QUOTA", "ERR_PROVIDER_QUOTA_OR_TIMEOUT"].includes(code)) {
    return "rate_limit";
  }
  if ([
    "ERR_PROVIDER_MODEL_INVALID",
    "ERR_PROVIDER_MODEL_UNSUPPORTED",
    "ERR_PROVIDER_UNSUPPORTED_CAPABILITY",
  ].includes(code)) return "model";
  if (["ERR_PROVIDER_RESPONSE", "ERR_PROVIDER_SCHEMA"].includes(code)) {
    return "response_format";
  }
  return "network";
}

export function App() {
  const [initializing, setInitializing] = useState(true);
  const [unlocked, setUnlocked] = useState(false);
  const [vaultExists, setVaultExists] = useState(false);
  const [route, setRoute] = useState<Route>({ name: "library" });
  const [projects, setProjects] = useState<MeetingProject[]>([]);
  const [providers, setProviders] = useState<ProviderDefinition[]>([]);
  const [statuses, setStatuses] = useState<ProviderConfigurationStatus[]>([]);
  const [activeDetail, setActiveDetail] = useState<ProjectDetail | null>(null);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const [languages, setLanguages] = useState<LanguagePreferences>(
    DEFAULT_LANGUAGE_PREFERENCES,
  );
  const [defaultProviderId, setDefaultProviderId] = useState("openai");
  const [inactivityMinutes, setInactivityMinutes] =
    useState<InactivityLockMinutes>(15);
  const lastActivity = useRef(Date.now());
  const hiddenAtRef = useRef<number | null>(null);
  const lockInFlight = useRef(false);

  const visibleProviders = useMemo(
    () => visibleProviderDefinitions(providers),
    [providers],
  );

  function releaseDefaultProvider(providerId: unknown) {
    return releaseSafeProviderId(providerId);
  }

  const clearUnlockedState = useCallback(
    (nextErrorCode: string | null = null, nextVaultExists?: boolean) => {
      setUnlocked(false);
      setProjects([]);
      setStatuses([]);
      setActiveDetail(null);
      setErrorCode(nextErrorCode);
      if (nextVaultExists !== undefined) setVaultExists(nextVaultExists);
    },
    [],
  );

  const requestLock = useCallback(
    async (trigger: LockTrigger) => {
      if (!unlocked || lockInFlight.current) return false;
      lockInFlight.current = true;
      try {
        await api.lock();
        clearUnlockedState();
        return true;
      } catch (error) {
        const code = lockErrorForTrigger(error, trigger);
        if (
          code !== "ERR_AUTO_LOCK_DEFERRED" &&
          code !== "ERR_AUTO_LOCK_DEFERRED_BUSY"
        ) {
          lastActivity.current = Date.now();
        }
        setErrorCode(code);
        return false;
      } finally {
        lockInFlight.current = false;
      }
    },
    [clearUnlockedState, unlocked],
  );

  useEffect(() => {
    void boot();
  }, []);

  useEffect(() => {
    if (!api.isNative || !unlocked) return;
    const refresh = () => void refreshAll();
    const realtimeRefresh = (event: { payload: RealtimeStateUpdate }) => {
      if (!activeDetail || event.payload.projectId === activeDetail.project.id) {
        void refreshAll();
      }
    };
    const vaultState = (event: {
      payload: {
        unlocked: boolean;
        errorCode?: string;
        vaultExists?: boolean;
      };
    }) => {
      if (!event.payload.unlocked) {
        clearUnlockedState(
          event.payload.errorCode ?? null,
          event.payload.vaultExists,
        );
      }
    };
    const unlisten: Array<() => void> = [];
    void Promise.all([
      listen("accordmesh://job-progress", refresh),
      listen("accordmesh://job-error", refresh),
      listen("accordmesh://timeline-final", refresh),
      listen("accordmesh://project-status", refresh),
      listen<RealtimeStateUpdate>(
        "accordmesh://realtime-state",
        realtimeRefresh,
      ),
      listen<{ unlocked: boolean; errorCode?: string; vaultExists?: boolean }>(
        "accordmesh://vault-state",
        vaultState,
      ),
    ]).then((values) => unlisten.push(...values));
    return () => unlisten.forEach((value) => value());
  }, [
    unlocked,
    activeDetail?.project.id,
    clearUnlockedState,
    requestLock,
  ]);

  useEffect(() => {
    lastActivity.current = Date.now();
    hiddenAtRef.current = null;
  }, [inactivityMinutes, unlocked]);

  useEffect(() => {
    if (!unlocked || inactivityMinutes === null) return;
    const activity = () => {
      lastActivity.current = Date.now();
    };
    const events = ["pointerdown", "keydown"] as const;
    events.forEach((name) => window.addEventListener(name, activity));
    const timer = window.setInterval(() => {
      const now = Date.now();
      if (shouldAutoLock(lastActivity.current, now, inactivityMinutes)) {
        void requestLock("inactivity");
      }
    }, 30_000);
    return () => {
      events.forEach((name) => window.removeEventListener(name, activity));
      window.clearInterval(timer);
    };
  }, [inactivityMinutes, requestLock, unlocked]);

  useEffect(() => {
    if (!unlocked || inactivityMinutes === null) return;

    const handleForeground = () => {
      if (document.hidden) return;
      const hiddenAt = hiddenAtRef.current ?? lastActivity.current;
      hiddenAtRef.current = null;
      if (Date.now() - hiddenAt > inactivityMinutes * 60_000) {
        void requestLock("background");
      } else {
        lastActivity.current = Date.now();
      }
    };

    const visibility = () => {
      if (document.hidden) {
        hiddenAtRef.current = Date.now();
        return;
      }
      handleForeground();
    };

    document.addEventListener("visibilitychange", visibility);
    window.addEventListener("focus", handleForeground);
    window.addEventListener("pageshow", handleForeground);
    return () => {
      document.removeEventListener("visibilitychange", visibility);
      window.removeEventListener("focus", handleForeground);
      window.removeEventListener("pageshow", handleForeground);
    };
  }, [inactivityMinutes, requestLock, unlocked]);

  async function boot() {
    try {
      const [status, definitions] = await Promise.all([
        api.setupStatus(),
        api.providerDefinitions(),
      ]);
      setVaultExists(status.vaultExists);
      setUnlocked(status.unlocked);
      setProviders(definitions);
      if (status.unlocked) await hydrate();
    } catch (error) {
      setErrorCode(String(error));
    } finally {
      setInitializing(false);
    }
  }

  async function hydrate() {
    const [projectList, providerStatuses, settings] = await Promise.all([
      api.listProjects(),
      api.providerConfigurationStatus(),
      api.loadSettings(),
    ]);
    setProjects(projectList);
    setStatuses(providerStatuses);
    setLanguages(normalizeLanguagePreferences(settings.languagePreferences));
    setDefaultProviderId(releaseDefaultProvider(settings.defaultProviderId));
    if (
      Object.prototype.hasOwnProperty.call(settings, "inactivityLockMinutes")
    ) {
      setInactivityMinutes(
        normalizeInactivityLockMinutes(settings.inactivityLockMinutes),
      );
    }
  }

  async function refreshAll() {
    await hydrate();
    if (activeDetail) {
      try {
        setActiveDetail(await api.getProjectDetail(activeDetail.project.id));
      } catch {
        setActiveDetail(null);
        setRoute({ name: "library" });
      }
    }
  }

  async function openProject(projectId: string) {
    const detail = await api.getProjectDetail(projectId);
    setActiveDetail(detail);
    setRoute({ name: "project", projectId });
    await hydrate();
  }

  async function onUnlocked() {
    const firstRun = !vaultExists;
    setUnlocked(true);
    setVaultExists(true);
    setErrorCode(null);
    hiddenAtRef.current = null;
    lastActivity.current = Date.now();
    await hydrate();
    if (firstRun) {
      setActiveDetail(null);
      setRoute({ name: "settings" });
      return;
    }
    if (route.name === "project") {
      try {
        const detail = await api.getProjectDetail(route.projectId);
        setActiveDetail(detail);
      } catch {
        setActiveDetail(null);
        setRoute({ name: "library" });
      }
    }
  }

  async function startRealtime(
    mode: RealtimeMode,
    title: string,
    deviceId: string,
    providerId: string,
  ) {
    const resolved = resolveLanguagePreferences(languages);
    const detail = await api.createRealtimeProject({
      mode,
      title:
        title.trim() ||
        t(
          mode === "online"
            ? "realtime.defaultOnlineTitle"
            : "realtime.defaultInPersonTitle",
        ),
      deviceId,
      sourceLanguage: resolved.sourceLanguage,
      translationTargetLanguage: resolved.translationTargetLanguage,
      analysisOutputLanguage: resolved.analysisOutputLanguage,
      providerId,
    });
    setActiveDetail(detail);
    setRoute({ name: "project", projectId: detail.project.id });
    await hydrate();
  }

  async function onProjectChanged(detail: ProjectDetail | null) {
    setActiveDetail(detail);
    await hydrate();
    if (!detail) setRoute({ name: "library" });
  }

  const nav = useMemo(
    () => [
      {
        key: "library",
        icon: "library" as IconName,
        label: t("library.title"),
        active: route.name === "library",
        action: () => setRoute({ name: "library" } as Route),
      },
      {
        key: "online",
        icon: "online" as IconName,
        label: t("library.newOnline"),
        active: route.name === "online",
        action: () => setRoute({ name: "online" } as Route),
      },
      {
        key: "inPerson",
        icon: "inPerson" as IconName,
        label: t("library.newInPerson"),
        active: route.name === "inPerson",
        action: () => setRoute({ name: "inPerson" } as Route),
      },
      {
        key: "upload",
        icon: "upload" as IconName,
        label: t("library.newUpload"),
        active: route.name === "upload",
        action: () => setRoute({ name: "upload" } as Route),
      },
      {
        key: "settings",
        icon: "settings" as IconName,
        label: t("settings.title"),
        active: route.name === "settings",
        action: () => setRoute({ name: "settings" } as Route),
      },
    ],
    [route.name],
  );

  if (initializing) {
    return (
      <main className="centered appLoadingState" role="status" aria-live="polite">
        <span className="loadingSpinner" aria-hidden="true" />
        <strong>{t("common.loading")}</strong>
      </main>
    );
  }
  if (!unlocked) {
    return (
      <UnlockPage
        vaultExists={vaultExists}
        onUnlocked={onUnlocked}
        onError={setErrorCode}
        errorCode={errorCode}
      />
    );
  }

  const resolvedLanguages = resolveLanguagePreferences(languages);
  const providerError = providerErrorCategory(errorCode);

  return (
    <div className="appFrame">
      <aside className="sidebar" aria-label={t("accessibility.mainNavigation")}>
        <div className="brandBlock">
          <span className="brandMark"><Icon name="brand" size={22} /></span>
          <div className="brandText">
            <strong>{t("common.appName")}</strong>
            <span>{t("common.tagline")}</span>
          </div>
        </div>
        {!api.isNative && (
          <div className="demoBadge">{t("unlock.demoModeShort")}</div>
        )}
        <nav className="sidebarNav">
          {nav.map((item) => (
            <button
              className={item.active ? "navButton active" : "navButton"}
              key={item.key}
              onClick={item.action}
              aria-current={item.active ? "page" : undefined}
            >
              <Icon name={item.icon} size={18} />
              <span>{item.label}</span>
            </button>
          ))}
        </nav>
        <div className="sidebarFooter">
          <button
            className="navButton lockButton"
            onClick={() => void requestLock("manual")}
          >
            <Icon name="lock" size={18} />
            <span>{t("settings.lockNow")}</span>
          </button>
          <span className="localOnlyLabel">{t("common.localOnly")}</span>
        </div>
      </aside>
      <main className="mainPane">
        <div className="mainContent">
        {errorCode && (
          <div className="notice error globalErrorNotice" role="alert">
            <div className="globalErrorCopy">
              <strong>{t(`errors.${errorCode}`)}</strong>
              {providerError && (
                <span>{t(`providers.errorGuidance.${providerError}`)}</span>
              )}
            </div>
            {providerError && (
              <div className="globalErrorActions">
                <button
                  type="button"
                  className="secondaryButton"
                  onClick={() => {
                    setErrorCode(null);
                    setRoute({ name: "settings" });
                  }}
                >
                  {t("providers.openSettingsAction")}
                </button>
                {activeDetail && (
                  <button
                    type="button"
                    className="secondaryButton"
                    onClick={() => {
                      setErrorCode(null);
                      setRoute({ name: "project", projectId: activeDetail.project.id });
                    }}
                  >
                    {t("providers.openMeetingAction")}
                  </button>
                )}
              </div>
            )}
            <button
              className="iconButton"
              type="button"
              aria-label={t("common.dismiss")}
              onClick={() => setErrorCode(null)}
            >
              <Icon name="close" size={15}/>
            </button>
          </div>
        )}
        {route.name === "library" && (
          <LibraryPage
            projects={projects}
            onOpen={openProject}
            onRefresh={hydrate}
            onError={setErrorCode}
          />
        )}
        {route.name === "online" && (
          <MeetingStartPage
            mode="online"
            providers={visibleProviders}
            providerStatuses={statuses}
            preferences={languages}
            defaultProviderId={defaultProviderId}
            onStart={startRealtime}
            onError={setErrorCode}
          />
        )}
        {route.name === "inPerson" && (
          <MeetingStartPage
            mode="in_person"
            providers={visibleProviders}
            providerStatuses={statuses}
            preferences={languages}
            defaultProviderId={defaultProviderId}
            onStart={startRealtime}
            onError={setErrorCode}
          />
        )}
        {route.name === "upload" && (
          <UploadPage
            key={route.attachId ?? "new"}
            projects={projects}
            providers={visibleProviders}
            providerStatuses={statuses}
            defaultProviderId={defaultProviderId}
            initialAttachId={route.attachId}
            preferences={languages}
            onCreated={(detail) => {
              setActiveDetail(detail);
              setRoute({ name: "project", projectId: detail.project.id });
              void hydrate();
            }}
            onError={setErrorCode}
          />
        )}
        {route.name === "project" && activeDetail && (
          <ProjectDetailPage
            detail={activeDetail}
            defaultProviderId={defaultProviderId}
            providers={visibleProviders}
            providerStatuses={statuses}
            analysisLanguage={resolvedLanguages.analysisOutputLanguage}
            onAttach={(projectId) => setRoute({ name: "upload", attachId: projectId })}
            onChanged={onProjectChanged}
            onError={setErrorCode}
          />
        )}
        {route.name === "settings" && (
          <SettingsPage
            providers={visibleProviders}
            providerStatuses={statuses}
            preferences={languages}
            defaultProviderId={defaultProviderId}
            inactivityMinutes={inactivityMinutes}
            onDefaultProvider={setDefaultProviderId}
            onInactivityMinutes={setInactivityMinutes}
            onPreferences={setLanguages}
            onRefreshStatuses={async () =>
              setStatuses(await api.providerConfigurationStatus())}
            onError={setErrorCode}
          />
        )}
        </div>
      </main>
    </div>
  );
}
