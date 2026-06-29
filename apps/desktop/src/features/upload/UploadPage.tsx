import { FormEvent, useEffect, useMemo, useState } from "react";
import { api } from "../../shared/api";
import type {
  LanguagePreferences,
  MeetingProject,
  ProjectDetail,
  ProviderConfigurationStatus,
  ProviderDefinition,
  SelectedFile,
} from "../../shared/types";
import { t } from "../../i18n";
import {
  providerLanguageCompatibility,
  resolveLanguagePreferences,
} from "../../shared/languagePreferences";
import {
  isAttachableRealtimeProject,
  isRecordingSelection,
} from "./attachmentEligibility";

interface Props {
  projects: MeetingProject[];
  providers: ProviderDefinition[];
  providerStatuses: ProviderConfigurationStatus[];
  defaultProviderId: string;
  initialAttachId?: string;
  preferences: LanguagePreferences;
  onCreated: (detail: ProjectDetail) => void;
  onError: (code: string | null) => void;
}

export function UploadPage({
  projects,
  providers,
  providerStatuses,
  defaultProviderId,
  initialAttachId,
  preferences,
  onCreated,
  onError,
}: Props) {
  const attachableProjects = useMemo(
    () => projects.filter(isAttachableRealtimeProject),
    [projects],
  );
  const initialAttachProjectId =
    initialAttachId &&
    attachableProjects.some((project) => project.id === initialAttachId)
      ? initialAttachId
      : "";
  const [mode, setMode] = useState<"new" | "attach">(
    initialAttachProjectId ? "attach" : "new",
  );
  const [title, setTitle] = useState("");
  const [selectedProject, setSelectedProject] = useState(
    initialAttachProjectId || attachableProjects[0]?.id || "",
  );
  const [file, setFile] = useState<SelectedFile | null>(null);
  const [providerId, setProviderId] = useState(defaultProviderId);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (mode !== "attach") return;
    if (attachableProjects.length === 0) {
      setMode("new");
      setSelectedProject("");
      return;
    }
    if (!attachableProjects.some((project) => project.id === selectedProject)) {
      setSelectedProject(attachableProjects[0].id);
    }
  }, [attachableProjects, mode, selectedProject]);

  const provider = providers.find((value) => value.id === providerId);
  const configured =
    providerId === "mock" ||
    providerStatuses.some(
      (value) => value.providerId === providerId && value.configured,
    );
  const resolvedLanguages = resolveLanguagePreferences(preferences);
  const languageCompatibility = providerLanguageCompatibility(
    provider?.capabilities,
    preferences,
  );

  function changeMode(nextMode: "new" | "attach") {
    if (nextMode === "attach" && attachableProjects.length === 0) return;
    setMode(nextMode);
    onError(null);
    if (nextMode === "attach") {
      if (!selectedProject) setSelectedProject(attachableProjects[0]?.id || "");
      if (file && !isRecordingSelection(file)) setFile(null);
    }
  }

  async function choose() {
    try {
      const selected = await api.selectFiles(
        mode === "attach" ? "recording" : "meeting_material",
      );
      if (selected.length !== 1) {
        onError("ERR_SINGLE_FILE_REQUIRED");
        return;
      }
      if (mode === "attach" && !isRecordingSelection(selected[0])) {
        onError("ERR_ATTACHMENT_MEDIA_REQUIRED");
        return;
      }
      setFile(selected[0]);
      onError(null);
    } catch (error) {
      if (String(error) !== "ERR_FILE_SELECTION_CANCELLED") {
        onError(String(error));
      }
    }
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!file) {
      onError("ERR_SINGLE_FILE_REQUIRED");
      return;
    }
    if (mode === "attach") {
      if (!isRecordingSelection(file)) {
        onError("ERR_ATTACHMENT_MEDIA_REQUIRED");
        return;
      }
      if (
        !selectedProject ||
        !attachableProjects.some((project) => project.id === selectedProject)
      ) {
        onError("ERR_ATTACHMENT_PROJECT_REQUIRED");
        return;
      }
    }

    setBusy(true);
    onError(null);
    try {
      const common = {
        files: [{ selectionToken: file.selectionToken }],
        sourceLanguage: resolvedLanguages.sourceLanguage,
        translationTargetLanguage: resolvedLanguages.translationTargetLanguage,
        analysisOutputLanguage: resolvedLanguages.analysisOutputLanguage,
        minutesOutputLanguage: resolvedLanguages.minutesOutputLanguage,
        providerId,
      };
      const detail =
        mode === "attach"
          ? await api.attachUpload({
              ...common,
              projectId: selectedProject,
            })
          : await api.createUploadProject({
              ...common,
              title: title.trim() || t("upload.defaultTitle"),
            });
      onCreated(detail);
    } catch (error) {
      onError(String(error));
    } finally {
      setBusy(false);
    }
  }

  const supported =
    !file ||
    Boolean(
      provider?.capabilities.supportedInputFormats.some(
        (format) =>
          format === file.kind ||
          file.originalFileName.toLowerCase().endsWith(`.${format}`),
      ),
    );
  const valid =
    Boolean(file) &&
    configured &&
    supported &&
    languageCompatibility.supported &&
    (mode === "new" ||
      (isRecordingSelection(file) &&
        attachableProjects.some((project) => project.id === selectedProject))) &&
    !busy;

  return (
    <section>
      <div className="pageHeader">
        <div>
          <h1>{t("upload.title")}</h1>
          <p>{t("upload.stages")}</p>
        </div>
      </div>
      <form className="formStack constrained" onSubmit={submit}>
        <div className="segmented">
          <button
            type="button"
            className={mode === "new" ? "active" : ""}
            onClick={() => changeMode("new")}
          >
            {t("upload.newProject")}
          </button>
          <button
            type="button"
            className={mode === "attach" ? "active" : ""}
            onClick={() => changeMode("attach")}
            disabled={attachableProjects.length === 0}
          >
            {t("upload.attachExisting")}
          </button>
        </div>
        {attachableProjects.length === 0 && (
          <div className="notice">{t("upload.noAttachableProjects")}</div>
        )}
        {mode === "new" ? (
          <label>
            {t("realtime.projectTitle")}
            <input
              value={title}
              onChange={(event) => setTitle(event.target.value)}
            />
          </label>
        ) : (
          <label>
            {t("upload.selectedProject")}
            <select
              value={selectedProject}
              onChange={(event) => setSelectedProject(event.target.value)}
              required
            >
              {attachableProjects.map((project) => (
                <option value={project.id} key={project.id}>
                  {project.title}
                </option>
              ))}
            </select>
          </label>
        )}
        <label>
          {t("common.provider")}
          <select
            value={providerId}
            onChange={(event) => setProviderId(event.target.value)}
          >
            {providers.map((value) => (
              <option key={value.id} value={value.id}>
                {t(value.displayNameKey)}
              </option>
            ))}
          </select>
        </label>
        {!configured && (
          <div className="notice error">
            {t("providers.configurationRequired")}
          </div>
        )}
        {!languageCompatibility.supported && languageCompatibility.errorCode && (
          <div className="notice error">
            {t(`errors.${languageCompatibility.errorCode}`)}
          </div>
        )}
        {!supported && (
          <div className="notice error">{t("upload.unsupportedSelection")}</div>
        )}
        {mode === "attach" && file && !isRecordingSelection(file) && (
          <div className="notice error">{t("upload.recordingFileOnly")}</div>
        )}
        <div className="filePicker">
          <button type="button" className="secondaryButton" onClick={choose}>
            {t(
              mode === "attach"
                ? file
                  ? "upload.replaceRecording"
                  : "upload.chooseRecording"
                : file
                  ? "upload.replaceFile"
                  : "upload.chooseFile",
            )}
          </button>
          {!file ? (
            <div className="emptyState">{t("upload.noFile")}</div>
          ) : (
            <div className="listPanel">
              <div className="rowItem">
                <div>
                  <strong>{file.originalFileName}</strong>
                  <span>
                    {t(
                      `upload.kind${file.kind[0].toUpperCase()}${file.kind.slice(1)}`,
                    )}{" "}
                    · {formatBytes(file.size)}
                  </span>
                </div>
                <button type="button" onClick={() => setFile(null)}>
                  {t("common.remove")}
                </button>
              </div>
            </div>
          )}
        </div>
        <div className="notice">
          {t(mode === "attach" ? "upload.singleRecordingNotice" : "upload.singleFileNotice")}
        </div>
        {!api.isNative && (
          <div className="notice">{t("upload.demoSelectionNotice")}</div>
        )}
        <button className="primaryButton" disabled={!valid}>
          {busy
            ? t("common.processing")
            : mode === "attach"
              ? t("upload.attachAndCompare")
              : t("upload.process")}
        </button>
      </form>
    </section>
  );
}

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}
