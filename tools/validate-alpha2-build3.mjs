import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const root = process.cwd();
let pass = 0;
let fail = 0;

function read(relative) {
  return fs.readFileSync(path.join(root, relative), "utf8");
}

function check(id, condition, message) {
  if (condition) {
    pass += 1;
    console.log(`[${id}] PASS - ${message}`);
  } else {
    fail += 1;
    console.error(`[${id}] FAIL - ${message}`);
  }
}

function includes(relative, needle) {
  return read(relative).includes(needle);
}

const lib = read("apps/desktop/src-tauri/src/lib.rs");
const audio = read("apps/desktop/src-tauri/src/audio/mod.rs");
const buildScript = read("apps/desktop/src-tauri/build.rs");
const gitignore = read(".gitignore");
const repository = read("apps/desktop/src-tauri/src/storage/repository.rs");
const jobs = read("apps/desktop/src-tauri/src/jobs/mod.rs");
const openai = read("apps/desktop/src-tauri/src/providers/openai/mod.rs");
const project = read("apps/desktop/src/features/project-detail/ProjectDetailPage.tsx");
const meeting = read("apps/desktop/src/features/online-meeting/MeetingStartPage.tsx");
const upload = read("apps/desktop/src/features/upload/UploadPage.tsx");
const library = read("apps/desktop/src/features/library/LibraryPage.tsx");
const settings = read("apps/desktop/src/features/settings/SettingsPage.tsx");
const app = read("apps/desktop/src/app/App.tsx");
const styles = read("apps/desktop/src/app/styles.css");
const title = read("apps/desktop/src/shared/projectTitle.ts");
const buildFlags = read("apps/desktop/src/shared/buildFlags.ts");
const providerUiRegistry = read("apps/desktop/src/shared/providerUiRegistry.ts");
const testAdapter = read("apps/desktop/src-tauri/src/providers/test_adapter.rs");
const providerRegistry = read("apps/desktop/src-tauri/src/providers/registry.rs");
const providersI18n = read("apps/desktop/src/i18n/locales/en/providers.json");
const settingsI18n = read("apps/desktop/src/i18n/locales/en/settings.json");
const unlockI18n = read("apps/desktop/src/i18n/locales/en/unlock.json");
const realtimeI18n = read("apps/desktop/src/i18n/locales/en/realtime.json");
const readme = read("README.md");
const buildFromSource = read("docs/BUILD_FROM_SOURCE.md");
const releaseChecklist = read("docs/RELEASE_CHECKLIST.md");
const exportRs = read("apps/desktop/src-tauri/src/export/mod.rs");
const contractTests = read("apps/desktop/src-tauri/src/public_contract_tests.rs");
const tauri = JSON.parse(read("apps/desktop/src-tauri/tauri.conf.json"));

check("P0-01A", audio.includes("SoundCheckControl") && audio.includes("cancel_and_wait"), "Sound Check has an owned cancellable runtime");
check("P0-01B", lib.includes("cancel_sound_check") && lib.includes("stop_active_sound_check"), "Lock/window shutdown can wait for microphone release");
check("P0-01C", meeting.includes("api.cancelSoundCheck()") && meeting.includes("checkingSound ? cancelSoundCheck() : soundCheck()"), "UI cancels Sound Check on route/device changes and by explicit action");
check("P0-01D", lib.includes("if let Ok(mut state) = state.lock()") && lib.includes("let _ = state.stop_active_sound_check();\n                };"), "Window-close microphone cleanup compiles with bounded lock temporaries");
check("P0-02", project.includes("projectPrimaryNavMore") && project.includes('id: "history"') && styles.includes("@media (max-width: 1180px)"), "Narrow workspace navigation exposes History through a visible More menu");
check("P0-03A", repository.includes("finalize_incomplete_media_for_job"), "Repository can move incomplete media to a terminal state");
check("P0-03B", jobs.includes('if attachment { "attached" } else { "failed" }') && lib.includes('if attach { "attached" } else { "failed" }'), "Queued and runtime upload failures converge attachment/media status");
check("P0-04A", openai.includes('DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1"'), "Official OpenAI default endpoint is /v1");
check("P0-04B", openai.includes("ERR_PROVIDER_OPENAI_BASE_URL") && openai.includes('parsed.path().trim_end_matches(\'/\') != "/v1"'), "Official OpenAI host rejects non-/v1 paths");
check("P0-04C", openai.includes("migrate_legacy_configuration") && lib.includes("migrate_provider_configuration_defaults"), "Exact legacy /v3 Vault value is migrated during unlock");
check("P0-04D", openai.includes('matches!(host, "127.0.0.1" | "localhost" | "::1")'), "Custom HTTPS and local provider endpoints remain supported");

check("P1-01", includes("apps/desktop/src/features/unlock/UnlockPage.tsx", "unlock.createTitle") && includes("apps/desktop/src/features/unlock/UnlockPage.tsx", "unlock.unlockTitle"), "First-run create and later unlock copy are distinct");
check("P1-02", app.includes('label: t("settings.title")'), "Main navigation names the complete page Settings");
check("P1-03", !project.includes('t("common.readOnly")'), "Contradictory global Read-only state is removed");
check("P1-04", styles.includes(".regenerationForm") && styles.includes("@media (max-width: 900px)") && styles.includes(".regenerationForm {\n    grid-template-columns: 1fr"), "Regenerate dialog stacks at narrow widths");
check("P1-05", styles.includes(".settingsWorkspace") && styles.includes("@media (max-width: 1100px)") && styles.includes(".settingsCategoryNav"), "Settings has narrow-window responsive rules");
check("P1-06", styles.includes("overflow-wrap: anywhere") && styles.includes(".exportFormatCard"), "Export cards allow safe text wrapping");
check("P1-07", styles.includes(".dangerZone") && styles.includes("white-space: normal"), "Reset and destructive actions wrap without clipping");
check("P1-08A", title.includes("PROJECT_TITLE_MAX_CHARS = 120") && lib.includes("PROJECT_TITLE_MAX_CHARS: usize = 120"), "Frontend and backend share the 120-character title limit");
check("P1-08B", !meeting.includes("maxLength={PROJECT_TITLE_MAX_CHARS}") && !upload.includes("maxLength={PROJECT_TITLE_MAX_CHARS}") && !library.includes("maxLength={PROJECT_TITLE_MAX_CHARS}"), "Title fields do not silently truncate over-limit input");
check("P1-08C", meeting.includes("titleOverLimit") && upload.includes("titleOverLimit") && library.includes("titleDraftOverLimit") && project.includes("titleDraftOverLimit"), "All title entry surfaces show and enforce explicit over-limit state");
check("P1-09", fs.existsSync(path.join(root, "apps/desktop/src/components/EmptyState.tsx")) && project.includes("<EmptyState"), "Reusable empty-state component is used in workspace content");
check("P1-10", settings.includes("providerStatusLabel") && includes("apps/desktop/src/i18n/locales/en/providers.json", "Saved · not verified"), "Provider status distinguishes saved configuration from verified availability");
check("P1-11", upload.includes("fileSelectionCard") && upload.includes("upload.browseFiles"), "Upload uses a descriptive custom file-selection card");
check("P1-12", includes("apps/desktop/src-tauri/src/providers/mock/mod.rs", "conditional pilot target") && !includes("apps/desktop/src-tauri/src/providers/mock/mod.rs", "Translation review scope affects budget"), "Mock transcript, analysis, and comparison semantics are aligned");
check("P1-13", project.includes("const previewSegments") && project.includes("sourceTranscript.trim().toLocaleLowerCase()") && project.includes("project.transcriptSources"), "Latest transcript includes source labeling and deduplication");
check("P1-14", project.includes("project.counts.") && library.includes("projectCountSingle"), "Visible statistics include explicit units");
check("P1-15", app.includes("providerErrorCategory") && includes("apps/desktop/src/i18n/locales/en/providers.json", "errorGuidance"), "Provider failures are categorized with actionable guidance");

check("C2-01", buildFlags.includes("VITE_ACCORDMESH_ENABLE_DEV_TOOLS") && buildFlags.includes("=== \"1\""), "Developer diagnostics are controlled by an explicit VITE build flag");
check("C2-02", app.includes('useState("openai")') && app.includes("visibleProviderDefinitions(providers)") && app.includes("releaseSafeProviderId(providerId)"), "Release UI uses the provider UI registry and defaults away from hidden providers");
check("C2-03", settings.includes("ENABLE_DEV_TOOLS") && settings.includes('id: "advanced"') && settings.includes("visibleProviderDefinitions(providers)") && settings.includes("providerSettingsPanelKind"), "Settings hides Developer diagnostics through the explicit provider UI registry");
check("C2-04", !settings.includes("providerEndpointPreview") && !settings.includes("onChange={(event) => setBaseUrl") && settings.includes("officialEndpointDescription"), "OpenAI public settings expose API key only and describe the managed official endpoint");
check("C2-05", settingsI18n.includes('"title": "Settings"') && providersI18n.includes("Official OpenAI endpoint managed by AccordMesh"), "Settings i18n keys and OpenAI managed-endpoint copy are present");
check("C2-06", readme.includes("VITE_ACCORDMESH_ENABLE_DEV_TOOLS=1") && readme.includes("Test Provider Adapter") && buildFromSource.includes("Developer diagnostics and MockProvider") && releaseChecklist.includes("Test Provider Adapter"), "README and release docs explain MockProvider, Test Provider Adapter visibility, and diagnostic builds");
check("C2-07", project.includes("window.setInterval") && project.includes("activeJobs.length") && jobs.includes('"accordmesh://project-status"') && jobs.includes("next_project_status"), "Upload failure convergence refreshes active projects and emits terminal project status");
check("C2-08", exportRs.includes("pub(crate) fn safe_name") && exportRs.includes("character.is_control()") && contractTests.includes("export_safe_name_preserves_unicode_meeting_titles"), "Export filenames preserve Unicode titles and replace only unsafe filename characters");
check("C2-09", styles.includes(".dialogProjectName") && styles.includes("max-height: 7.25rem") && styles.includes("overflow: auto"), "Delete confirmation long-title display is scroll-bounded rather than vertically clipped");
check("C2-10", realtimeI18n.includes("Cancel sound check") && meeting.includes("realtime.cancelSoundCheck"), "Sound Check cancel button has a real localized label");


check("UIP-01", providerUiRegistry.includes("ProviderUiDefinition") && providerUiRegistry.includes("visibility: \"production\"") && providerUiRegistry.includes("visibility: \"developer\""), "Provider Selector visibility is frontend-registry driven");
check("UIP-02", providerUiRegistry.includes("TEST_PROVIDER_ADAPTER_ID") && providerUiRegistry.includes("test_adapter") && providerUiRegistry.includes("isRuntimeProvider: false"), "Test Provider Adapter is registered as UI-only in the frontend registry");
check("UIP-03", providerUiRegistry.includes("visibleProviderDefinitions") && providerUiRegistry.includes("ENABLE_DEV_TOOLS") && providerUiRegistry.includes("ui.visibility === \"production\" || ENABLE_DEV_TOOLS"), "Public UI exposes only production providers unless dev tools are explicitly enabled");
check("UIP-04", providerUiRegistry.includes("releaseSafeProviderId") && providerUiRegistry.includes("!ENABLE_DEV_TOOLS && (isDevOnlyProvider(value) || isUiOnlyProvider(value))"), "Release default-provider normalization rejects dev-only and UI-only providers only in public builds");
check("UIP-05", settings.includes("providers.activeProvider") && settings.includes("setActiveProviderId") && settings.includes("activeProviderPanelKind"), "Settings page has a Provider Selector separate from OpenAI-specific credential controls");
check("UIP-06", settings.includes("ProviderModelAssignments") && settings.includes("provider.modelAssignments.map") && !settings.includes("openAiDefinition.modelAssignments.map"), "Models by task render from the active provider definition");
check("UIP-07", settings.includes("ProviderCapabilitySection") && settings.includes("Object.entries(provider.capabilities)") && !settings.includes("visibleProviders.map((provider) => (\n                  <details"), "Capabilities render from the active provider definition");
check("UIP-08", testAdapter.includes("ERR_TEST_PROVIDER_ADAPTER_UI_ONLY") && providerRegistry.includes("test_adapter::ID => Err(test_adapter::UI_ONLY_ERROR)") && providerRegistry.includes("test_adapter::validate_configuration"), "Test Provider Adapter fails closed for runtime and configuration paths");
check("UIP-09", settings.includes("rangeNumberControl") && settings.includes("numberUnitInput") && settings.includes("OVERLAY_FONT_SIZE_MIN") && settings.includes("OVERLAY_OPACITY_MAX"), "Appearance controls include synchronized numeric inputs for font size and opacity");
check(
  "UIP-12",
  settings.includes("overlayFontSizeInput") &&
    settings.includes("overlayOpacityInput") &&
    settings.includes("parseIntegerDraft") &&
    settings.includes("commitOverlayFontSizeInput") &&
    settings.includes("commitOverlayOpacityInput") &&
    settings.includes("setOverlayFontSizeInput(event.target.value)") &&
    settings.includes("setOverlayOpacityInput(event.target.value)") &&
    settings.includes("onBlur={() => commitOverlayFontSizeInput(true)}") &&
    settings.includes("onBlur={() => commitOverlayOpacityInput(true)}"),
  "Appearance numeric inputs keep editable draft text and commit bounded values on blur",
);
check(
  "UIP-13",
  settings.includes("defaultProviderOptions = useMemo(") &&
    settings.includes("isUiOnlyProvider(provider.id)") &&
    settings.includes("providers.uiExtensionTestOnly") &&
    settings.includes("disabled={!ready}") &&
    !settings.includes("disabled={!selectable || !ready}"),
  "Default provider selector exposes dev-only UI adapters when dev tools are enabled",
);
check(
  "UIP-14",
  project.includes("regenerationProviderChoices") &&
    project.includes("visibleProviderDefinitions(providers)") &&
    project.includes("isUiOnlyProvider(definition.id)") &&
    project.includes("syntheticProviderStatus(definition)") &&
    project.includes("providers.uiExtensionTestOnly") &&
    !project.includes("disabled={!selectable}"),
  "Regenerate provider selector exposes UI-only test adapters while runtime remains fail-closed",
);
check(
  "UIP-15",
  includes("apps/desktop/src/features/unlock/UnlockPage.tsx", "showPassword") &&
    includes("apps/desktop/src/features/unlock/UnlockPage.tsx", "passwordRevealTextButton") &&
    unlockI18n.includes("showPassword") &&
    unlockI18n.includes("hidePassword"),
  "Unlock and create-vault password fields can reveal or hide entered text",
);
check(
  "UIP-16",
  includes("apps/desktop/src/features/unlock/UnlockPage.tsx", "passwordRevealTextButton") &&
    includes("apps/desktop/src/features/unlock/UnlockPage.tsx", 't(showPassword ? "unlock.hidePassword" : "unlock.showPassword")') &&
    includes("apps/desktop/src/features/unlock/UnlockPage.tsx", 't(showConfirmPassword ? "unlock.hidePassword" : "unlock.showPassword")') &&
    includes("apps/desktop/src/app/styles.css", ".passwordField") &&
    includes("apps/desktop/src/app/styles.css", "display: flex") &&
    includes("apps/desktop/src/app/styles.css", ".passwordRevealTextButton") &&
    includes("apps/desktop/src/app/styles.css", "min-width: 72px") &&
    includes("apps/desktop/src/app/styles.css", "height: 40px") &&
    includes("apps/desktop/src/app/styles.css", "transform: none") &&
    !includes("apps/desktop/src/features/unlock/UnlockPage.tsx", 'name={showPassword ? "eyeOff" : "eye"}'),
  "Password reveal controls use a stable text button beside the password field",
);
check("UIP-10", providersI18n.includes("Test Provider Adapter") && providersI18n.includes("UI extension test only") && settingsI18n.includes('"units"'), "i18n resources include Test Provider Adapter and numeric unit copy");
check("UIP-11", readme.includes("Adding a Provider Adapter") && includes("docs/PROVIDER_EXTENSION_GUIDE.md", "UI registration") && includes("docs/AI_PROVIDER_ARCHITECTURE.md", "Test Provider Adapter"), "Provider Adapter extension and UI-only test adapter are documented");

check("BUILD-01", tauri.version === "0.1.0", "CFBundleShortVersionString source remains 0.1.0");
check("BUILD-02", tauri.bundle?.macOS?.bundleVersion === "3", "Internal macOS build number is 3");
check("BUILD-03", tauri.app?.windows?.[0]?.minWidth === 1024 && tauri.app?.windows?.[0]?.minHeight === 700, "Minimum window contract is 1024 × 700");
check("SAFE-01", !openai.includes("store=true"), "No opt-in OpenAI response storage was introduced");
check("SAFE-02", includes("apps/desktop/src-tauri/tauri.macos.conf.json", '"externalBin"'), "Bundled media sidecars remain declared");
check("BUILD-04", buildScript.includes("is_development_stub") && buildScript.includes('profile != "release"'), "Debug/test builds may reuse only the exact generated media-runtime stub");
check("BUILD-05", gitignore.includes("apps/desktop/src-tauri/gen/"), "Tauri generated schemas are excluded from the tracked source-integrity tree");

console.log(`ACCORDMESH ALPHA2 BUILD3 CORRECTIVE2 UI POLISH1 CONTRACT SUMMARY pass=${pass} fail=${fail}`);
process.exit(fail === 0 ? 0 : 1);
