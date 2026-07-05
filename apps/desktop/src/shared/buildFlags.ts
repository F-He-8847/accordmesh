type BuildImportMeta = ImportMeta & {
  env?: Record<string, string | boolean | undefined>;
};

const buildEnv = (import.meta as BuildImportMeta).env ?? {};

export const ENABLE_DEV_TOOLS =
  buildEnv.VITE_ACCORDMESH_ENABLE_DEV_TOOLS === "1";
