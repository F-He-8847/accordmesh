import { ENABLE_DEV_TOOLS } from "./buildFlags";
import type { ProviderDefinition } from "./types";

export type ProviderVisibility = "production" | "developer";
export type ProviderSettingsPanelKind =
  | "openaiApiKey"
  | "developerDiagnostics"
  | "uiExtensionTest";

export interface ProviderUiDefinition {
  id: string;
  visibility: ProviderVisibility;
  descriptionKey: string;
  settingsPanelKind: ProviderSettingsPanelKind;
  canBeDefaultProvider: boolean;
  isRuntimeProvider: boolean;
}

export const TEST_PROVIDER_ADAPTER_ID = "test_adapter";

const providerUiDefinitions: Record<string, ProviderUiDefinition> = {
  openai: {
    id: "openai",
    visibility: "production",
    descriptionKey: "providers.openAiDescription",
    settingsPanelKind: "openaiApiKey",
    canBeDefaultProvider: true,
    isRuntimeProvider: true,
  },
  mock: {
    id: "mock",
    visibility: "developer",
    descriptionKey: "providers.mockDescription",
    settingsPanelKind: "developerDiagnostics",
    canBeDefaultProvider: true,
    isRuntimeProvider: true,
  },
  [TEST_PROVIDER_ADAPTER_ID]: {
    id: TEST_PROVIDER_ADAPTER_ID,
    visibility: "developer",
    descriptionKey: "providers.testAdapter.description",
    settingsPanelKind: "uiExtensionTest",
    canBeDefaultProvider: true,
    isRuntimeProvider: false,
  },
};

export function providerUiDefinition(
  providerId: string,
): ProviderUiDefinition | undefined {
  return providerUiDefinitions[providerId];
}

export function providerDescriptionKey(providerId: string): string {
  return (
    providerUiDefinition(providerId)?.descriptionKey ??
    "providers.providerAdapterGenericDescription"
  );
}

export function providerSettingsPanelKind(
  providerId: string,
): ProviderSettingsPanelKind {
  return providerUiDefinition(providerId)?.settingsPanelKind ?? "uiExtensionTest";
}

export function isDevOnlyProvider(providerId: string): boolean {
  return providerUiDefinition(providerId)?.visibility === "developer";
}

export function isUiOnlyProvider(providerId: string): boolean {
  return providerUiDefinition(providerId)?.isRuntimeProvider === false;
}

export function canUseAsDefaultProvider(providerId: string): boolean {
  const ui = providerUiDefinition(providerId);
  if (!ui) return ENABLE_DEV_TOOLS;
  return ui.canBeDefaultProvider;
}

export function visibleProviderDefinitions(
  providers: ProviderDefinition[],
): ProviderDefinition[] {
  return providers.filter((provider) => {
    const ui = providerUiDefinition(provider.id);
    if (!ui) return ENABLE_DEV_TOOLS;
    return ui.visibility === "production" || ENABLE_DEV_TOOLS;
  });
}

export function releaseSafeProviderId(providerId: unknown): string {
  const value = typeof providerId === "string" && providerId.trim()
    ? providerId.trim()
    : "openai";
  if (!ENABLE_DEV_TOOLS && (isDevOnlyProvider(value) || isUiOnlyProvider(value))) {
    return "openai";
  }
  return value;
}
