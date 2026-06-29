import { useEffect, useMemo, useRef, useState } from "react";
import type { JSX, ReactNode } from "react";
import { api } from "../../shared/api";
import type {
  AnalysisArtifact,
  ExportFormat,
  MeetingProject,
  ProjectDetail,
  ProviderConfigurationStatus,
  ProviderDefinition,
  ProcessingJob,
  TimelineSegment,
} from "../../shared/types";
import { t } from "../../i18n";
import { Dialog } from "../../components/Dialog";
import { Icon } from "../../components/Icon";
import type { IconName } from "../../components/Icon";
import {
  artifactSelectionKey,
  groupArtifactsBySelectionScope,
  groupArtifactsByType,
  selectedArtifactIdsForExport,
} from "./artifactVersionSelection";
import {
  buildSourceMediaById,
  buildUntimedParagraphNumbers,
} from "./transcriptPresentation";
import type { SourceMediaAssetLike } from "./transcriptPresentation";
import {
  classifyTranscriptEvidence,
  mediaForSegment,
  partitionTranscriptEvidence,
} from "./transcriptEvidence";
import type {
  TranscriptEvidenceKind,
  TranscriptEvidenceView,
} from "./transcriptEvidence";
import {
  isAttachableRealtimeProject,
  isRealtimeProject,
} from "../upload/attachmentEligibility";
import {
  canDeleteProject,
  deleteGuardKey,
} from "../library/projectDeletion";
import { LANGUAGE_CODES, languageLabelKey } from "../../shared/languagePreferences";

interface Props {
  detail: ProjectDetail;
  defaultProviderId: string;
  providers: ProviderDefinition[];
  providerStatuses: ProviderConfigurationStatus[];
  analysisLanguage: string;
  onAttach: (projectId: string) => void;
  onChanged: (detail: ProjectDetail | null) => Promise<void>;
  onError: (code: string | null) => void;
}

type WorkspaceTab =
  | "overview"
  | "transcript"
  | "analysis"
  | "minutes"
  | "comparison"
  | "files"
  | "history";
type TranscriptTab = "sourceTranscript" | "translations" | "timeline";
type AnalysisTab = "understanding" | "communicationReview";

type RegeneratableArtifactType =
  | "literal_translation"
  | "segment_understanding"
  | "post_meeting_analysis"
  | "communication_review"
  | "intelligent_comparison_report"
  | "meeting_minutes";

interface RegenerationRequest {
  type: RegeneratableArtifactType;
  artifact: AnalysisArtifact;
}

interface PendingRegenerationSelection {
  type: RegeneratableArtifactType;
  selectionKey: string;
  baselineArtifactIds: string[];
}

const workspaceTabs: Array<{ id: WorkspaceTab; icon: IconName }> = [
  { id: "overview", icon: "library" },
  { id: "transcript", icon: "media" },
  { id: "analysis", icon: "analyze" },
  { id: "minutes", icon: "minutes" },
  { id: "comparison", icon: "comparison" },
  { id: "files", icon: "upload" },
  { id: "history", icon: "sort" },
];
const transcriptTabs: TranscriptTab[] = [
  "sourceTranscript",
  "translations",
  "timeline",
];
const analysisTabs: AnalysisTab[] = [
  "understanding",
  "communicationReview",
];

export function ProjectDetailPage({
  detail,
  defaultProviderId,
  providers,
  providerStatuses,
  analysisLanguage,
  onAttach,
  onChanged,
  onError,
}: Props) {
  const [workspaceTab, setWorkspaceTab] = useState<WorkspaceTab>("overview");
  const [transcriptTab, setTranscriptTab] =
    useState<TranscriptTab>("sourceTranscript");
  const [analysisTab, setAnalysisTab] =
    useState<AnalysisTab>("understanding");
  const [transcriptView, setTranscriptView] =
    useState<TranscriptEvidenceView>("both");
  const [sessionBusy, setSessionBusy] = useState(false);
  const [sessionMessage, setSessionMessage] = useState<string | null>(null);
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleDraft, setTitleDraft] = useState(detail.project.title);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [confirmingStop, setConfirmingStop] = useState(false);
  const [exportReady, setExportReady] = useState(false);
  const[includeTranscript,setIncludeTranscript]=useState(false);const[exportPath,setExportPath]=useState("");
  const [exportFormat, setExportFormat] = useState<ExportFormat>("markdown");
  const [exportBusy, setExportBusy] = useState(false);
  const [exportConfirmOpen, setExportConfirmOpen] = useState(false);
  const [exportFeedback, setExportFeedback] = useState<"success" | "cancelled" | null>(null);
  const [titleBusy, setTitleBusy] = useState(false);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const [version, setVersion] = useState<Record<string, string>>({});
  const [regenerationRequest, setRegenerationRequest] = useState<RegenerationRequest | null>(null);
  const [regenerationProviderId, setRegenerationProviderId] = useState("");
  const [regenerationModelId, setRegenerationModelId] = useState("");
  const [regenerationLanguage, setRegenerationLanguage] = useState(analysisLanguage);
  const [regenerationBusy, setRegenerationBusy] = useState(false);
  const regenerationSubmitRef = useRef(false);
  const regenerationProviderIdRef = useRef("");
  const regenerationModelIdRef = useRef("");
  const regenerationLanguageRef = useRef(analysisLanguage);
  const regenerationRequestIdRef = useRef("");
  const [regenerationRequestId, setRegenerationRequestId] = useState("");
  const [pendingRegenerationSelection, setPendingRegenerationSelection] =
    useState<PendingRegenerationSelection | null>(null);
  const [retryingJobId, setRetryingJobId] = useState<string | null>(null);

  useEffect(() => {
    setTitleDraft(detail.project.title);
  }, [detail.project.title]);
  useEffect(() => {
    if (!sessionMessage) return;
    const timer = window.setTimeout(() => setSessionMessage(null), 4_000);
    return () => window.clearTimeout(timer);
  }, [sessionMessage]);

  useEffect(() => {
    if (!pendingRegenerationSelection) return;
    const baseline = new Set(pendingRegenerationSelection.baselineArtifactIds);
    const candidates = detail.artifacts
      .filter(
        (artifact) =>
          artifact.artifactType === pendingRegenerationSelection.type &&
          artifactSelectionKey(artifact) === pendingRegenerationSelection.selectionKey &&
          !baseline.has(artifact.id) &&
          isUsableArtifact(artifact),
      )
      .sort(compareArtifacts);
    const latest = candidates.at(-1);
    if (latest) {
      setVersion((current) => ({
        ...current,
        [pendingRegenerationSelection.selectionKey]: latest.id,
      }));
      setPendingRegenerationSelection(null);
      return;
    }
    const regenerationStillActive = detail.jobs.some(
      (job) =>
        job.kind === "regenerate" &&
        ["queued", "running", "resumable", "cancelling"].includes(job.status),
    );
    if (!regenerationStillActive) setPendingRegenerationSelection(null);
  }, [detail.artifacts, detail.jobs, pendingRegenerationSelection]);

  const byType = useMemo(
    () => groupArtifactsByType(detail.artifacts),
    [detail.artifacts],
  );
  const sourceMediaById = useMemo(
    () => buildSourceMediaById(detail.mediaAssets),
    [detail.mediaAssets],
  );
  const untimedParagraphNumbers = useMemo(
    () => buildUntimedParagraphNumbers(detail.timeline, sourceMediaById),
    [detail.timeline, sourceMediaById],
  );
  const hasUntimedTranscript = untimedParagraphNumbers.size > 0;
  const transcriptEvidence = useMemo(
    () => partitionTranscriptEvidence(detail.project.origin, detail.timeline),
    [detail.project.origin, detail.timeline],
  );
  const selectedArtifactIds = useMemo(
    () => selectedArtifactIdsForExport(byType, version),
    [byType, version],
  );
  const canAttachRecording = isAttachableRealtimeProject(detail.project);
  const hasAttachedRecording =
    isRealtimeProject(detail.project) &&
    (detail.mediaAssets.length > 0 || Boolean(detail.project.hasComparison));
  const deleteGuard = deleteGuardKey(detail.project,detail.jobs);
  const canDelete = canDeleteProject(detail.project, detail.jobs);
  const artifactCount = detail.artifacts.filter(
    (artifact) => artifact.status === "completed",
  ).length;
  const activeJobs = useMemo(
    () => detail.jobs.filter((job) =>
      ["queued", "running", "resumable", "cancelling"].includes(job.status),
    ),
    [detail.jobs],
  );
  const failedJobs = useMemo(
    () => detail.jobs.filter((job) => ["failed", "cancelled"].includes(job.status)),
    [detail.jobs],
  );

  async function refresh() {
    await onChanged(await api.getProjectDetail(detail.project.id));
  }

  async function rename() {
    const title = titleDraft.trim();
    if (!title || title === detail.project.title || titleBusy) {
      setEditingTitle(false);
      return;
    }
    setTitleBusy(true);
    try {
      await api.renameProject(detail.project.id, title);
      await refresh();
      setEditingTitle(false);
      onError(null);
    } catch (error) {
      onError(String(error));
    } finally {
      setTitleBusy(false);
    }
  }

  async function remove() {
    if (!canDelete || deleteBusy) return;
    setDeleteBusy(true);
    try {
      await api.deleteProject(detail.project.id);
      await onChanged(null);
      onError(null);
    } catch (error) {
      onError(String(error));
      setDeleteBusy(false);
    }
  }

  async function exportFile(format: ExportFormat) {
    if (exportBusy) return;
    setExportBusy(true);
    setExportFeedback(null);
    try {
      setExportPath(
        await api.exportProject(
          detail.project.id,
          format,
          selectedArtifactIds,
          format === "json" || includeTranscript,
        ),
      );
      setExportFeedback("success");
      setExportConfirmOpen(false);
      onError(null);
    } catch (error) {
      if (String(error) === "ERR_EXPORT_CANCELLED") {
        setExportFeedback("cancelled");
        setExportConfirmOpen(false);
      } else {
        onError(String(error));
      }
    } finally {
      setExportBusy(false);
    }
  }

  function openRegeneration(type: string, artifact: AnalysisArtifact) {
    if (!isRegeneratableArtifactType(type)) return;
    const compatible = compatibleRegenerationProviders(
      type,
      providers,
      providerStatuses,
    );
    const initialProvider =
      compatible.find((provider) => provider.definition.id === artifact.providerId) ??
      compatible.find((provider) => provider.definition.id === defaultProviderId) ??
      compatible[0];
    const initialLanguage = artifactOutputLanguage(artifact) ?? analysisLanguage;
    const initialProviderId = initialProvider?.definition.id ?? "";
    const initialModelId = initialProvider
      ? regenerationModelOptions(
          type,
          artifact,
          initialProvider.definition,
          initialProvider.status,
        )[0]?.id ?? ""
      : "";
    const requestId = window.crypto.randomUUID();

    regenerationProviderIdRef.current = initialProviderId;
    regenerationModelIdRef.current = initialModelId;
    regenerationLanguageRef.current = initialLanguage;
    regenerationRequestIdRef.current = requestId;

    setRegenerationRequest({ type, artifact });
    setRegenerationRequestId(requestId);
    setRegenerationProviderId(initialProviderId);
    setRegenerationModelId(initialModelId);
    setRegenerationLanguage(initialLanguage);
    onError(null);
  }

  async function regenerate() {
    const selectedProviderId =
      regenerationProviderIdRef.current || regenerationProviderId;
    const selectedModelId =
      regenerationModelIdRef.current || regenerationModelId;
    const selectedLanguage =
      regenerationLanguageRef.current || regenerationLanguage;
    const requestId =
      regenerationRequestIdRef.current || regenerationRequestId;

    if (
      !regenerationRequest ||
      regenerationBusy ||
      regenerationSubmitRef.current ||
      !selectedProviderId ||
      !requestId
    ) return;

    const { type, artifact } = regenerationRequest;
    const sources = regenerationSources(type, artifact, byType, version);
    if (!sources) {
      onError("ERR_JOB_PAYLOAD");
      return;
    }

    const selectionKey = artifactSelectionKey(artifact);
    setPendingRegenerationSelection({
      type,
      selectionKey,
      baselineArtifactIds: detail.artifacts
        .filter(
          (item) =>
            item.artifactType === type &&
            artifactSelectionKey(item) === selectionKey,
        )
        .map((item) => item.id),
    });
    regenerationSubmitRef.current = true;
    setRegenerationBusy(true);
    try {
      await api.regenerateArtifact({
        requestId,
        projectId: detail.project.id,
        artifactType: type,
        providerId: selectedProviderId,
        modelId: selectedModelId || undefined,
        outputLanguage: selectedLanguage,
        sourceSegmentIds: sources.sourceSegmentIds,
        sourceArtifactIds: sources.sourceArtifactIds,
      });
      setRegenerationRequest(null);
      setRegenerationRequestId("");
      regenerationRequestIdRef.current = "";
      await refresh();
      onError(null);
    } catch (error) {
      setPendingRegenerationSelection(null);
      onError(String(error));
    } finally {
      regenerationSubmitRef.current = false;
      setRegenerationBusy(false);
    }
  }

  async function retryProcessingJob(job: ProcessingJob) {
    if (retryingJobId || job.errorCode === "ERR_JOB_PAYLOAD") return;
    setRetryingJobId(job.id);
    try {
      await api.retryJob(job.id);
      await refresh();
      onError(null);
    } catch (error) {
      onError(String(error));
    } finally {
      setRetryingJobId(null);
    }
  }

  async function sessionAction(
    action: "overlay" | "pause" | "resume" | "analyze" | "stop",
  ) {
    if (sessionBusy) return;
    setSessionBusy(true);
    setSessionMessage(null);
    try {
      if (action === "overlay") await api.showOverlay(detail.project.id);
      if (action === "pause") await api.pauseRealtime(detail.project.id);
      if (action === "resume") await api.resumeRealtime(detail.project.id);
      if (action === "analyze") {
        await api.analyzeNow(detail.project.id);
        setSessionMessage(t("realtime.analyzeRequested"));
      }
      if (action === "stop") {
        await api.stopRealtime(detail.project.id);
        onError(null);
      }
      if (action !== "overlay") await refresh();
    } catch (error) {
      const code = String(error);
      if (action === "stop" && code === "ERR_REALTIME_STOP_TIMEOUT") {
        try { await refresh(); } catch { /* preserve the stop error */ }
      }
      onError(code);
    } finally {
      setSessionBusy(false);
    }
  }

  function openWorkspace(tab: WorkspaceTab) {
    setWorkspaceTab(tab);
  }

  return (
    <section className="projectWorkspace">
      <header className="projectDetailHeader">
        <div className="projectIdentity">
          <span className="pageEyebrow">{t("project.workspaceEyebrow")}</span>
          <div className="projectTitleRow">
            <h1 title={detail.project.title}>{detail.project.title}</h1>
            <span className={`statusBadge status-${detail.project.status}`}>
              {t(`common.${detail.project.status}`)}
            </span>
          </div>
          <div className="projectMetaLine">
            <span>{originLabel(detail.project.origin)}</span>
            <span>{new Date(detail.project.createdAt).toLocaleString()}</span>
            <span>{t("common.readOnly")}</span>
          </div>
        </div>
        <div className="headerActions projectHeaderActions">
          {canAttachRecording&&<button className="primaryButton" onClick={()=>onAttach(detail.project.id)}><Icon name="media" size={16}/>{t("project.attachRecording")}</button>}
          {hasAttachedRecording&&<span className="statusPill"><Icon name="media" size={14}/>{t("project.recordingAttached")}</span>}
          <button onClick={() => { setTitleDraft(detail.project.title); setEditingTitle(true); }}>
            <Icon name="rename" size={16}/>
            {t("common.rename")}
          </button>
          <button className="dangerButton" disabled={!canDelete} title={deleteGuard ? t(deleteGuard) : undefined} onClick={() => setConfirmingDelete(true)}>
            <Icon name="delete" size={16}/>
            {t("common.delete")}
          </button>
        </div>
      </header>

      {deleteGuard && <div className="notice">{t(deleteGuard)}</div>}

      {detail.realtimeSession &&
        ["running", "paused"].includes(detail.realtimeSession.status) && (
          <div className="sessionBar projectSessionBar">
            <div>
              <span className="liveDot" />
              <strong>
                {detail.realtimeSession.status === "paused"
                  ? t("common.paused")
                  : t("realtime.activeNotice")}
              </strong>
            </div>
            <div className="buttonRow">
              <button disabled={sessionBusy} onClick={() => void sessionAction("overlay")}>
                {t("realtime.showOverlay")}
              </button>
              {detail.realtimeSession.status === "running" ? (
                <button disabled={sessionBusy} onClick={() => void sessionAction("pause")}>
                  <Icon name="pause" size={16}/>
                  {t("common.pause")}
                </button>
              ) : (
                <button disabled={sessionBusy} onClick={() => void sessionAction("resume")}>
                  <Icon name="resume" size={16}/>
                  {t("common.resume")}
                </button>
              )}
              <button disabled={sessionBusy} onClick={() => void sessionAction("analyze")}>
                <Icon name="analyze" size={16}/>
                {t("realtime.analyzeNow")}
              </button>
              <button disabled={sessionBusy} className="dangerButton" onClick={() => setConfirmingStop(true)}>
                <Icon name="stop" size={16}/>
                {t("realtime.stop")}
              </button>
            </div>
          </div>
        )}
      {sessionMessage && (
        <div className="notice" role="status" aria-live="polite">
          {sessionMessage}
        </div>
      )}

      {activeJobs.map((job) => (
        <div className="notice" key={job.id}>
          <div className="jobRow">
            <span>
              {t(`project.jobStages.${job.stage}`)} · {Math.round(job.progress * 100)}%
            </span>
            <progress max={1} value={job.progress} />
            <div className="buttonRow">
              {["queued", "running", "resumable"].includes(job.status) && (
                <button onClick={async () => { await api.cancelJob(job.id); await refresh(); }}>
                  {t("common.cancel")}
                </button>
              )}
              {job.status === "resumable" && (
                <button onClick={() => void retryProcessingJob(job)}>
                  {t("common.retry")}
                </button>
              )}
            </div>
          </div>
          {job.errorCode && <span>{t(`errors.${job.errorCode}`)}</span>}
        </div>
      ))}
      {failedJobs.length > 0 && workspaceTab !== "history" && (
        <div className="notice jobHistoryNotice" role="status">
          <span>{t("project.failedProcessingSummary", { count: failedJobs.length })}</span>
          <button type="button" onClick={() => setWorkspaceTab("history")}>
            {t("project.reviewHistory")}
          </button>
        </div>
      )}

      <nav className="projectPrimaryNav" aria-label={t("project.workspaceNavigation")}>
        {workspaceTabs.map((item) => (
          <button
            key={item.id}
            className={workspaceTab === item.id ? "active" : ""}
            aria-current={workspaceTab === item.id ? "page" : undefined}
            onClick={() => openWorkspace(item.id)}
          >
            <Icon name={item.icon} size={17}/>
            <span>{t(`project.workspaceTabs.${item.id}`)}</span>
          </button>
        ))}
      </nav>

      <div className="projectContentShell">
        {workspaceTab === "overview" && (
          <OverviewPanel
            detail={detail}
            artifactCount={artifactCount}
            onOpen={openWorkspace}
          />
        )}

        {workspaceTab === "transcript" && (
          <WorkspaceSection
            title={t("project.workspaceTabs.transcript")}
            description={t("project.workspaceDescriptions.transcript")}
          >
            <div className="projectSecondaryNav" role="tablist">
              {transcriptTabs.map((item) => (
                <button
                  key={item}
                  className={transcriptTab === item ? "active" : ""}
                  role="tab"
                  aria-selected={transcriptTab === item}
                  onClick={() => setTranscriptTab(item)}
                >
                  {t(`project.${item}`)}
                </button>
              ))}
            </div>
            {transcriptTab === "sourceTranscript" && (
              <TranscriptEvidencePanel
                project={detail.project}
                sourceMediaById={sourceMediaById}
                untimedParagraphNumbers={untimedParagraphNumbers}
                partition={transcriptEvidence}
                view={transcriptView}
                onViewChange={setTranscriptView}
              />
            )}
            {transcriptTab === "translations" && (
              <ArtifactVersions
                type="literal_translation"
                artifacts={byType.literal_translation}
                selectedVersionByScope={version}
                onSelect={(artifact) => setVersion((current) => ({ ...current, [artifactSelectionKey(artifact)]: artifact.id }))}
                onRegenerate={openRegeneration}
                timeline={detail.timeline}
              />
            )}
            {transcriptTab === "timeline" && (
              <TimelinePanel
                detail={detail}
                hasUntimedTranscript={hasUntimedTranscript}
                sourceMediaById={sourceMediaById}
                untimedParagraphNumbers={untimedParagraphNumbers}
              />
            )}
          </WorkspaceSection>
        )}

        {workspaceTab === "analysis" && (
          <WorkspaceSection
            title={t("project.workspaceTabs.analysis")}
            description={t("project.workspaceDescriptions.analysis")}
          >
            <div className="projectSecondaryNav" role="tablist">
              {analysisTabs.map((item) => (
                <button
                  key={item}
                  className={analysisTab === item ? "active" : ""}
                  role="tab"
                  aria-selected={analysisTab === item}
                  onClick={() => setAnalysisTab(item)}
                >
                  {t(`project.${item}`)}
                </button>
              ))}
            </div>
            {analysisTab === "understanding" && (
              <div className="analysisUnderstandingLayout">
                <ArtifactVersions
                  type="post_meeting_analysis"
                  artifacts={byType.post_meeting_analysis}
                  selectedVersionByScope={version}
                  onSelect={(artifact) => setVersion((current) => ({ ...current, [artifactSelectionKey(artifact)]: artifact.id }))}
                  onRegenerate={openRegeneration}
                  timeline={detail.timeline}
                />
                <SegmentInsightsPanel
                  artifacts={byType.segment_understanding}
                  selectedVersionByScope={version}
                  onSelect={(artifact) => setVersion((current) => ({ ...current, [artifactSelectionKey(artifact)]: artifact.id }))}
                  onRegenerate={openRegeneration}
                  timeline={detail.timeline}
                />
              </div>
            )}
            {analysisTab === "communicationReview" && (
              <ArtifactVersions
                type="communication_review"
                artifacts={byType.communication_review}
                selectedVersionByScope={version}
                onSelect={(artifact) => setVersion((current) => ({ ...current, [artifactSelectionKey(artifact)]: artifact.id }))}
                onRegenerate={openRegeneration}
                timeline={detail.timeline}
              />
            )}
          </WorkspaceSection>
        )}

        {workspaceTab === "minutes" && (
          <WorkspaceSection
            title={t("project.workspaceTabs.minutes")}
            description={t("project.workspaceDescriptions.minutes")}
          >
            <ArtifactVersions
              type="meeting_minutes"
              artifacts={byType.meeting_minutes}
              selectedVersionByScope={version}
              onSelect={(artifact) => setVersion((current) => ({ ...current, [artifactSelectionKey(artifact)]: artifact.id }))}
              onRegenerate={openRegeneration}
              timeline={detail.timeline}
            />
          </WorkspaceSection>
        )}

        {workspaceTab === "comparison" && (
          <WorkspaceSection
            title={t("project.workspaceTabs.comparison")}
            description={t("project.workspaceDescriptions.comparison")}
          >
            <ArtifactVersions
              type="intelligent_comparison_report"
              artifacts={byType.intelligent_comparison_report}
              selectedVersionByScope={version}
              onSelect={(artifact) => setVersion((current) => ({ ...current, [artifactSelectionKey(artifact)]: artifact.id }))}
              onRegenerate={openRegeneration}
              timeline={detail.timeline}
            />
          </WorkspaceSection>
        )}

        {workspaceTab === "files" && (
          <WorkspaceSection
            title={t("project.workspaceTabs.files")}
            description={t("project.workspaceDescriptions.files")}
          >
            <FilesPanel
              detail={detail}
              exportReady={exportReady}
              setExportReady={setExportReady}
              includeTranscript={includeTranscript}
              setIncludeTranscript={setIncludeTranscript}
              exportPath={exportPath}
              exportFormat={exportFormat}
              setExportFormat={setExportFormat}
              exportBusy={exportBusy}
              exportFeedback={exportFeedback}
              onRequestExport={() => setExportConfirmOpen(true)}
            />
          </WorkspaceSection>
        )}

        {workspaceTab === "history" && (
          <WorkspaceSection
            title={t("project.workspaceTabs.history")}
            description={t("project.workspaceDescriptions.history")}
          >
            <HistoryPanel
              detail={detail}
              failedJobs={failedJobs}
              retryingJobId={retryingJobId}
              onRetryJob={retryProcessingJob}
            />
          </WorkspaceSection>
        )}
      </div>

      <Dialog
        open={editingTitle}
        title={t("library.renameDialogTitle")}
        description={t("library.renameDialogDescription")}
        closeLabel={t("common.close")}
        onClose={() => {
          if (!titleBusy) setEditingTitle(false);
        }}
        actions={
          <>
            <button type="button" disabled={titleBusy} onClick={() => setEditingTitle(false)}>
              {t("common.cancel")}
            </button>
            <button
              type="button"
              className="primaryButton"
              disabled={!titleDraft.trim() || titleBusy}
              onClick={() => void rename()}
            >
              {titleBusy ? t("common.saving") : t("common.save")}
            </button>
          </>
        }
      >
        <label className="dialogField">
          <span>{t("library.renamePlaceholder")}</span>
          <input
            value={titleDraft}
            maxLength={240}
            autoFocus
            onChange={(event) => setTitleDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void rename();
            }}
          />
        </label>
      </Dialog>

      <Dialog
        open={confirmingDelete && canDelete}
        title={t("library.deleteDialogTitle")}
        description={t("library.deleteConfirm", { projectName: detail.project.title })}
        tone="irreversible"
        closeLabel={t("common.close")}
        onClose={() => {
          if (!deleteBusy) setConfirmingDelete(false);
        }}
        actions={
          <>
            <button type="button" disabled={deleteBusy} onClick={() => setConfirmingDelete(false)}>
              {t("common.cancel")}
            </button>
            <button type="button" className="dangerButton" disabled={deleteBusy} onClick={() => void remove()}>
              {deleteBusy ? t("common.processing") : t("common.delete")}
            </button>
          </>
        }
      />

      <Dialog
        open={confirmingStop}
        title={t("realtime.stopConfirmTitle")}
        description={t("realtime.stopConfirmBody")}
        tone="danger"
        closeLabel={t("common.close")}
        onClose={() => {
          if (!sessionBusy) setConfirmingStop(false);
        }}
        actions={
          <>
            <button type="button" disabled={sessionBusy} onClick={() => setConfirmingStop(false)}>
              {t("common.cancel")}
            </button>
            <button
              type="button"
              className="dangerButton"
              disabled={sessionBusy}
              onClick={async () => {
                await sessionAction("stop");
                setConfirmingStop(false);
              }}
            >
              {sessionBusy ? t("common.processing") : t("realtime.stopAndFinalize")}
            </button>
          </>
        }
      />

      <Dialog
        open={Boolean(regenerationRequest)}
        title={
          regenerationRequest
            ? regenerationDialogTitle(regenerationRequest.type)
            : t("project.regeneration.title")
        }
        description={t("project.regeneration.description")}
        closeLabel={t("common.close")}
        onClose={() => {
          if (!regenerationBusy) {
            setRegenerationRequest(null);
            setRegenerationRequestId("");
            regenerationRequestIdRef.current = "";
          }
        }}
        className="regenerationDialog"
        actions={
          <>
            <button
              type="button"
              disabled={regenerationBusy}
              onClick={() => {
                setRegenerationRequest(null);
                setRegenerationRequestId("");
              }}
            >
              {t("common.cancel")}
            </button>
            <button
              type="button"
              className="primaryButton"
              disabled={
                regenerationBusy ||
                !regenerationProviderId ||
                !regenerationModelId ||
                !regenerationLanguage
              }
              onClick={() => void regenerate()}
            >
              {regenerationBusy
                ? t("common.processing")
                : regenerationRequest
                  ? regenerationActionLabel(regenerationRequest.type)
                  : t("project.regeneration.submit")}
            </button>
          </>
        }
      >
        {regenerationRequest && (
          <RegenerationControls
            request={regenerationRequest}
            providers={providers}
            providerStatuses={providerStatuses}
            providerId={regenerationProviderId}
            modelId={regenerationModelId}
            outputLanguage={regenerationLanguage}
            onProviderChange={(providerId) => {
              const option = compatibleRegenerationProviders(
                regenerationRequest.type,
                providers,
                providerStatuses,
              ).find((provider) => provider.definition.id === providerId);
              const modelId = option
                ? regenerationModelOptions(
                    regenerationRequest.type,
                    regenerationRequest.artifact,
                    option.definition,
                    option.status,
                  )[0]?.id ?? ""
                : "";

              regenerationProviderIdRef.current = providerId;
              regenerationModelIdRef.current = modelId;
              setRegenerationProviderId(providerId);
              setRegenerationModelId(modelId);
            }}
            onModelChange={(modelId) => {
              regenerationModelIdRef.current = modelId;
              setRegenerationModelId(modelId);
            }}
            onLanguageChange={(language) => {
              regenerationLanguageRef.current = language;
              setRegenerationLanguage(language);
            }}
          />
        )}
      </Dialog>

      <Dialog
        open={exportConfirmOpen}
        title={t("project.exportConfirmTitle")}
        description={t("project.exportConfirmDescription", {
          format: exportFormat.toUpperCase(),
        })}
        tone="danger"
        closeLabel={t("common.close")}
        onClose={() => {
          if (!exportBusy) setExportConfirmOpen(false);
        }}
        actions={
          <>
            <button type="button" disabled={exportBusy} onClick={() => setExportConfirmOpen(false)}>
              {t("common.cancel")}
            </button>
            <button type="button" className="primaryButton" disabled={exportBusy} onClick={() => void exportFile(exportFormat)}>
              {exportBusy ? t("common.processing") : t("common.export")}
            </button>
          </>
        }
      >
        <div className="dialogExportSummary">
          <Icon name={exportFormat === "json" ? "shield" : "minutes"} size={19}/>
          <div>
            <strong>{t(`project.exportFormats.${exportFormat}.title`)}</strong>
            <p>
              {exportFormat === "json"
                ? t("project.jsonAuditNote")
                : includeTranscript
                  ? t("project.exportIncludesTranscript")
                  : t("project.exportExcludesTranscript")}
            </p>
          </div>
        </div>
      </Dialog>
    </section>
  );
}

function WorkspaceSection({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <section className="workspaceSection">
      <header className="workspaceSectionHeader">
        <div>
          <h2>{title}</h2>
          <p>{description}</p>
        </div>
      </header>
      {children}
    </section>
  );
}

function OverviewPanel({
  detail,
  artifactCount,
  onOpen,
}: {
  detail: ProjectDetail;
  artifactCount: number;
  onOpen: (tab: WorkspaceTab) => void;
}) {
  const outputs = [
    {
      id: "transcript" as const,
      icon: "media" as IconName,
      count: detail.timeline.length,
      ready: detail.timeline.length > 0,
    },
    {
      id: "analysis" as const,
      icon: "analyze" as IconName,
      count: detail.artifacts.filter((artifact) =>
        ["segment_understanding", "post_meeting_analysis", "communication_review"].includes(artifact.artifactType),
      ).length,
      ready: artifactCount > 0,
    },
    {
      id: "minutes" as const,
      icon: "minutes" as IconName,
      count: detail.artifacts.filter((artifact) => artifact.artifactType === "meeting_minutes").length,
      ready: Boolean(detail.project.hasMinutes),
    },
    {
      id: "comparison" as const,
      icon: "comparison" as IconName,
      count: detail.artifacts.filter((artifact) => artifact.artifactType === "intelligent_comparison_report").length,
      ready: Boolean(detail.project.hasComparison),
    },
  ];
  return (
    <div className="overviewLayout">
      <section className="overviewMetricGrid" aria-label={t("project.projectSummary")}>
        <Info
          label={t("project.origin")}
          value={originLabel(detail.project.origin)}
          icon={originIcon(detail.project.origin)}
        />
        <Info
          label={t("common.status")}
          value={t(`common.${detail.project.status}`)}
          icon="shield"
          status={detail.project.status}
        />
        <Info
          label={t("project.created")}
          value={new Date(detail.project.createdAt).toLocaleString()}
          icon="sort"
        />
        <Info
          label={t("project.timeline")}
          value={String(detail.timeline.length)}
          icon="media"
          numeric
        />
        <Info
          label={t("project.materials")}
          value={String(detail.mediaAssets.length)}
          icon="upload"
          numeric
        />
        <Info
          label={t("project.generatedArtifacts")}
          value={String(artifactCount)}
          icon="analyze"
          numeric
        />
      </section>
      <section className="overviewOutputSection">
        <div className="sectionTitleRow">
          <div>
            <h2>{t("project.availableOutputs")}</h2>
            <p>{t("project.availableOutputsDescription")}</p>
          </div>
        </div>
        <div className="outputCardGrid">
          {outputs.map((output) => (
            <button
              className="outputCard"
              key={output.id}
              onClick={() => onOpen(output.id)}
            >
              <span className={`outputIcon output-${output.id}`}>
                <Icon name={output.icon} size={19}/>
              </span>
              <span className="outputCardText">
                <strong>{t(`project.workspaceTabs.${output.id}`)}</strong>
                <small>
                  {output.ready
                    ? t("project.outputReady", { count: output.count })
                    : t("project.outputNotReady")}
                </small>
              </span>
              <Icon name="open" size={16}/>
            </button>
          ))}
        </div>
      </section>
      <section className="overviewEvidencePreview">
        <div className="sectionTitleRow">
          <div>
            <h2>{t("project.recentEvidence")}</h2>
            <p>{t("project.recentEvidenceDescription")}</p>
          </div>
          <button onClick={() => onOpen("transcript")}>{t("project.openTranscript")}</button>
        </div>
        {detail.timeline.length ? (
          <div className="evidencePreviewList">
            {detail.timeline.slice(-3).map((segment) => (
              <article key={segment.id}>
                <span>{formatTime(segment.startMs)}</span>
                <p>{segment.sourceTranscript}</p>
              </article>
            ))}
          </div>
        ) : (
          <div className="emptyState">{t("project.noTranscriptEvidence")}</div>
        )}
      </section>
    </div>
  );
}

function TimelinePanel({
  detail,
  hasUntimedTranscript,
  sourceMediaById,
  untimedParagraphNumbers,
}: {
  detail: ProjectDetail;
  hasUntimedTranscript: boolean;
  sourceMediaById: ReadonlyMap<string, SourceMediaAssetLike>;
  untimedParagraphNumbers: ReadonlyMap<string, number>;
}) {
  return (
    <div className="listPanel timelinePanel">
      {hasUntimedTranscript && (
        <div className="notice">{t("project.untimedTranscriptNotice")}</div>
      )}
      {detail.timeline.map((segment) => {
        const paragraphNumber = untimedParagraphNumbers.get(segment.id);
        const sourceMedia = mediaForSegment(segment, sourceMediaById);
        const evidenceKind = classifyTranscriptEvidence(detail.project.origin, segment);
        return (
          <div className={`timelineItem evidence-${evidenceKind}`} key={segment.id}>
            <span>
              {paragraphNumber
                ? `${sourceMedia?.originalFileName ?? t("project.untimedTextFile")} · ${t("project.paragraph", { number: paragraphNumber })}`
                : `${formatTime(segment.startMs)}-${formatTime(segment.endMs)}`}
            </span>
            <div className="timelineSource">
              <strong>{t(`project.transcriptSources.${evidenceKind}`)}</strong>
              <small>
                {sourceMedia
                  ? `${t(`project.trackRoles.${segment.trackRole}`)} · ${sourceMedia.originalFileName}`
                  : t(`project.trackRoles.${segment.trackRole}`)}
              </small>
            </div>
            <p>{segment.sourceTranscript}</p>
          </div>
        );
      })}
    </div>
  );
}

function MaterialsPanel({ detail }: { detail: ProjectDetail }) {
  if (!detail.mediaAssets.length) {
    return <div className="emptyState">{t("project.noMaterials")}</div>;
  }
  return (
    <div className="materialGrid">
      {detail.mediaAssets.map((asset) => (
        <article className="materialCard" key={asset.id}>
          <span className="materialIcon"><Icon name="media" size={19}/></span>
          <div>
            <strong>{asset.originalFileName}</strong>
            <span>
              {t(`upload.kind${asset.kind[0].toUpperCase()}${asset.kind.slice(1)}`)} · {displayMediaStatus(asset.processingStatus)}
            </span>
            <small>{asset.sha256}</small>
          </div>
        </article>
      ))}
    </div>
  );
}

function FilesPanel({
  detail,
  exportReady,
  setExportReady,
  includeTranscript,
  setIncludeTranscript,
  exportPath,
  exportFormat,
  setExportFormat,
  exportBusy,
  exportFeedback,
  onRequestExport,
}: {
  detail: ProjectDetail;
  exportReady: boolean;
  setExportReady: (ready: boolean) => void;
  includeTranscript: boolean;
  setIncludeTranscript: (include: boolean) => void;
  exportPath: string;
  exportFormat: ExportFormat;
  setExportFormat: (format: ExportFormat) => void;
  exportBusy: boolean;
  exportFeedback: "success" | "cancelled" | null;
  onRequestExport: () => void;
}) {
  return (
    <div className="filesWorkspaceLayout">
      <section className="historyCard filesSourceCard">
        <div className="sectionTitleRow">
          <div>
            <h3>{t("project.materials")}</h3>
            <p>{t("project.filesSourceDescription")}</p>
          </div>
          <span className="countBadge">{detail.mediaAssets.length}</span>
        </div>
        <MaterialsPanel detail={detail}/>
      </section>
      <ExportPanel
        exportReady={exportReady}
        setExportReady={setExportReady}
        includeTranscript={includeTranscript}
        setIncludeTranscript={setIncludeTranscript}
        exportPath={exportPath}
        exportFormat={exportFormat}
        setExportFormat={setExportFormat}
        exportBusy={exportBusy}
        exportFeedback={exportFeedback}
        onRequestExport={onRequestExport}
      />
    </div>
  );
}

function ExportPanel({
  exportReady,
  setExportReady,
  includeTranscript,
  setIncludeTranscript,
  exportPath,
  exportFormat,
  setExportFormat,
  exportBusy,
  exportFeedback,
  onRequestExport,
}: {
  exportReady: boolean;
  setExportReady: (ready: boolean) => void;
  includeTranscript: boolean;
  setIncludeTranscript: (include: boolean) => void;
  exportPath: string;
  exportFormat: ExportFormat;
  setExportFormat: (format: ExportFormat) => void;
  exportBusy: boolean;
  exportFeedback: "success" | "cancelled" | null;
  onRequestExport: () => void;
}) {
  const formats: Array<{ id: ExportFormat; icon: IconName }> = [
    { id: "markdown", icon: "minutes" },
    { id: "txt", icon: "media" },
    { id: "json", icon: "shield" },
  ];
  return (
    <section className="historyCard exportWorkspaceCard">
      <div className="sectionTitleRow">
        <div>
          <h3>{t("project.exportSectionTitle")}</h3>
          <p>{t("project.humanReadableExportDescription")}</p>
        </div>
      </div>
      <div className="notice exportPlaintextNotice">
        <Icon name="shield" size={17}/>
        <span>{t("project.exportWarning")}</span>
      </div>
      {!exportReady ? (
        <button className="primaryButton" onClick={() => setExportReady(true)}>
          {t("project.acknowledgeExport")}
        </button>
      ) : (
        <div className="exportPanel">
          <fieldset className="exportFormatFieldset">
            <legend>{t("project.chooseExportFormat")}</legend>
            <div className="exportFormatGrid">
              {formats.map((format) => (
                <label
                  className={`exportFormatCard ${exportFormat === format.id ? "selected" : ""}`}
                  key={format.id}
                >
                  <input
                    type="radio"
                    name="export-format"
                    value={format.id}
                    checked={exportFormat === format.id}
                    onChange={() => setExportFormat(format.id)}
                  />
                  <span className="exportFormatIcon"><Icon name={format.icon} size={18}/></span>
                  <span className="exportFormatCopy">
                    <strong>{t(`project.exportFormats.${format.id}.title`)}</strong>
                    <small>{t(`project.exportFormats.${format.id}.description`)}</small>
                  </span>
                </label>
              ))}
            </div>
          </fieldset>
          <label className={`checkboxRow exportTranscriptOption ${exportFormat === "json" ? "disabled" : ""}`}>
            <input
              type="checkbox"
              checked={exportFormat === "json" || includeTranscript}
              disabled={exportFormat === "json"}
              onChange={(event) => setIncludeTranscript(event.target.checked)}
            />
            <span>
              <strong>{t("project.includeFullTranscript")}</strong>
              <small>
                {exportFormat === "json"
                  ? t("project.jsonAlwaysIncludesTranscript")
                  : t("project.transcriptOptionDescription")}
              </small>
            </span>
          </label>
          <button className="primaryButton exportActionButton" disabled={exportBusy} onClick={onRequestExport}>
            <Icon name="upload" size={16}/>
            {exportBusy
              ? t("common.processing")
              : t("project.exportSelectedFormat", {
                  format: t(`project.exportFormats.${exportFormat}.title`),
                })}
          </button>
          <span className="exportLegacyLabels" aria-hidden="true">
            {t("project.markdownReport")} · {t("project.txtReport")} · {t("project.jsonAudit")}
          </span>
        </div>
      )}
      {exportFeedback === "success" && exportPath && (
        <div className="notice successNotice exportFeedback" role="status" aria-live="polite">
          <Icon name="shield" size={17}/>
          <span>{t("project.exportPath", { path: exportPath })}</span>
        </div>
      )}
      {exportFeedback === "cancelled" && (
        <div className="notice exportFeedback" role="status" aria-live="polite">
          {t("project.exportCancelled")}
        </div>
      )}
    </section>
  );
}

function HistoryPanel({
  detail,
  failedJobs,
  retryingJobId,
  onRetryJob,
}: {
  detail: ProjectDetail;
  failedJobs: ProcessingJob[];
  retryingJobId: string | null;
  onRetryJob: (job: ProcessingJob) => Promise<void>;
}) {
  const orderedRuns = [...detail.generationRuns].sort((left, right) =>
    right.createdAt.localeCompare(left.createdAt),
  );
  const completedRuns = orderedRuns.filter((run) => run.status === "completed").length;
  const activeJobs = detail.jobs.filter((job) =>
    ["queued", "running", "resumable", "cancelling"].includes(job.status),
  ).length;
  return (
    <div className="historyLayout">
      <section className="historySummaryGrid" aria-label={t("project.historySummary")}>
        <HistoryMetric label={t("project.historyMetrics.activity")} value={orderedRuns.length}/>
        <HistoryMetric label={t("project.historyMetrics.completed")} value={completedRuns}/>
        <HistoryMetric label={t("project.historyMetrics.attention")} value={failedJobs.length} tone={failedJobs.length ? "warning" : undefined}/>
        <HistoryMetric label={t("project.historyMetrics.active")} value={activeJobs}/>
      </section>
      {failedJobs.length > 0 && (
        <section className="historyCard historyAttentionCard">
          <div className="sectionTitleRow">
            <div>
              <h3>{t("project.failedProcessingAttempts")}</h3>
              <p>{t("project.failedProcessingDescription")}</p>
            </div>
            <span className="countBadge">{failedJobs.length}</span>
          </div>
          <div className="failedJobList">
            {failedJobs.map((job) => (
              <article className="failedJobItem" key={job.id}>
                <div className="failedJobCopy">
                  <strong>{jobKindLabel(job.kind)}</strong>
                  <span>{new Date(job.createdAt).toLocaleString()}</span>
                  {job.errorCode && <span className="errorText">{t(`errors.${job.errorCode}`)}</span>}
                  {job.errorCode === "ERR_JOB_PAYLOAD" && (
                    <small>{t("project.invalidSavedRequestHint")}</small>
                  )}
                </div>
                {job.errorCode !== "ERR_JOB_PAYLOAD" && (
                  <button
                    type="button"
                    disabled={retryingJobId === job.id}
                    onClick={() => void onRetryJob(job)}
                  >
                    {retryingJobId === job.id
                      ? t("common.processing")
                      : t("project.retryThisRun")}
                  </button>
                )}
              </article>
            ))}
          </div>
        </section>
      )}
      <section className="historyCard historyActivityCard">
        <div className="sectionTitleRow">
          <div>
            <h3>{t("project.generationHistory")}</h3>
            <p>{t("project.generationHistoryDescription")}</p>
          </div>
          <span className="countBadge">{orderedRuns.length}</span>
        </div>
        <div className="generationRunList">
          {orderedRuns.length ? orderedRuns.map((run) => (
            <article className="generationRunItem" key={run.id}>
              <div>
                <strong>{run.schemaVersion}</strong>
                <span>{displayProviderName(run.providerId)} · {displayModelName(run.modelId)}</span>
                <small>{new Date(run.createdAt).toLocaleString()}</small>
              </div>
              <span className={`statusBadge status-${run.status}`}>{displayRunStatus(run.status)}</span>
              {run.errorCode && <span className="errorText">{t(`errors.${run.errorCode}`)}</span>}
            </article>
          )) : <div className="emptyState">{t("project.noGenerationHistory")}</div>}
        </div>
      </section>
    </div>
  );
}

function HistoryMetric({
  label,
  value,
  tone,
}: {
  label: string;
  value: number;
  tone?: "warning";
}) {
  return (
    <article className={`historyMetric ${tone ? `historyMetric-${tone}` : ""}`}>
      <span>{label}</span>
      <strong>{value}</strong>
    </article>
  );
}

function TranscriptEvidencePanel({
  project,
  sourceMediaById,
  untimedParagraphNumbers,
  partition,
  view,
  onViewChange,
}: {
  project: MeetingProject;
  sourceMediaById: ReadonlyMap<string, SourceMediaAssetLike>;
  untimedParagraphNumbers: ReadonlyMap<string, number>;
  partition: ReturnType<typeof partitionTranscriptEvidence>;
  view: TranscriptEvidenceView;
  onViewChange: (view: TranscriptEvidenceView) => void;
}) {
  const effectiveView = partition.hasComparableSources ? view : "both";
  const sections: Array<{
    kind: TranscriptEvidenceKind;
    segments: TimelineSegment[];
  }> = [];
  if (project.origin === "upload_only") {
    if (partition.uploaded.length)
      sections.push({ kind: "uploaded", segments: partition.uploaded });
    if (partition.unknown.length)
      sections.push({ kind: "unknown", segments: partition.unknown });
  } else {
    if (
      (effectiveView === "both" || effectiveView === "realtime") &&
      partition.realtime.length
    )
      sections.push({ kind: "realtime", segments: partition.realtime });
    if (
      (effectiveView === "both" || effectiveView === "recording") &&
      partition.recording.length
    )
      sections.push({ kind: "recording", segments: partition.recording });
    if (effectiveView === "both" && partition.unknown.length)
      sections.push({ kind: "unknown", segments: partition.unknown });
  }
  return (
    <div className="transcriptEvidencePanel">
      {partition.hasComparableSources && (
        <div className="transcriptEvidenceToolbar" role="group" aria-label={t("project.transcriptViewLabel")}>
          {(["both","realtime","recording"] as TranscriptEvidenceView[]).map((option) => (
            <button
              key={option}
              className={effectiveView === option ? "active" : ""}
              aria-pressed={effectiveView===option}
              onClick={() => onViewChange(option)}
            >
              {t(`project.transcriptViews.${option}`)}
            </button>
          ))}
        </div>
      )}
      <div className={`transcriptEvidenceGrid ${sections.length === 1 ? "single" : ""}`}>
        {sections.map(section=><TranscriptEvidenceSection key={section.kind} kind={section.kind} segments={section.segments} sourceMediaById={sourceMediaById} untimedParagraphNumbers={untimedParagraphNumbers}/>) }
      </div>
    </div>
  );
}

function TranscriptEvidenceSection({
  kind,
  segments,
  sourceMediaById,
  untimedParagraphNumbers,
}: {
  kind: TranscriptEvidenceKind;
  segments: TimelineSegment[];
  sourceMediaById: ReadonlyMap<string, SourceMediaAssetLike>;
  untimedParagraphNumbers: ReadonlyMap<string, number>;
}) {
  return (
    <section className={`transcriptEvidenceSection evidence-${kind}`}>
      <header>
        <div>
          <h2>{t(`project.transcriptSources.${kind}`)}</h2>
          <p>{t(`project.transcriptSourceDescriptions.${kind}`)}</p>
        </div>
        <span className="sourceBadge">{segments.length}</span>
      </header>
      <div className="transcriptEvidenceList">
        {segments.map((segment) => {
          const sourceMedia = mediaForSegment(segment, sourceMediaById);
          const paragraphNumber = untimedParagraphNumbers.get(segment.id);
          const location = paragraphNumber
            ? `${sourceMedia?.originalFileName ?? t("project.untimedTextFile")} · ${t("project.paragraph", { number: paragraphNumber })}`
            : `${formatTime(segment.startMs)}-${formatTime(segment.endMs)}`;
          return (
            <article className="transcriptEvidenceItem" key={segment.id}>
              <div className="transcriptEvidenceMeta">
                <strong>{location}</strong>
                <span>
                  {sourceMedia
                    ? `${t(`project.trackRoles.${segment.trackRole}`)} · ${sourceMedia.originalFileName}`
                    : t(`project.trackRoles.${segment.trackRole}`)}
                </span>
                {segment.detectedLanguage && <small>{displayLanguageName(segment.detectedLanguage)}</small>}
              </div>
              <p>{segment.sourceTranscript}</p>
            </article>
          );
        })}
      </div>
    </section>
  );
}

function RegenerationControls({
  request,
  providers,
  providerStatuses,
  providerId,
  modelId,
  outputLanguage,
  onProviderChange,
  onModelChange,
  onLanguageChange,
}: {
  request: RegenerationRequest;
  providers: ProviderDefinition[];
  providerStatuses: ProviderConfigurationStatus[];
  providerId: string;
  modelId: string;
  outputLanguage: string;
  onProviderChange: (providerId: string) => void;
  onModelChange: (modelId: string) => void;
  onLanguageChange: (language: string) => void;
}) {
  const compatible = compatibleRegenerationProviders(
    request.type,
    providers,
    providerStatuses,
  );
  const selected = compatible.find(
    (provider) => provider.definition.id === providerId,
  );
  const models = selected
    ? regenerationModelOptions(
        request.type,
        request.artifact,
        selected.definition,
        selected.status,
      )
    : [];
  const sourceLabel = `${artifactTypeLabel(request.type)} · ${new Date(
    request.artifact.createdAt,
  ).toLocaleString()}`;

  return (
    <div className="regenerationForm">
      <div className="regenerationSourceSummary">
        <span>{t("project.regeneration.basedOn")}</span>
        <strong>{sourceLabel}</strong>
        {request.type === "meeting_minutes" && (
          <small>{t("project.regeneration.minutesSourcesNotice")}</small>
        )}
      </div>
      {compatible.length ? (
        <>
          <label className="dialogField">
            <span>{t("project.regeneration.provider")}</span>
            <select
              value={providerId}
              onChange={(event) => onProviderChange(event.target.value)}
            >
              {compatible.map(({ definition }) => (
                <option key={definition.id} value={definition.id}>
                  {t(definition.displayNameKey)}
                </option>
              ))}
            </select>
            <small>
              {selected?.definition.id === "mock"
                ? t("project.regeneration.mockProviderAvailable")
                : t("project.regeneration.configuredNotVerified")}
            </small>
          </label>
          <label className="dialogField">
            <span>{t("project.regeneration.model")}</span>
            <select
              value={modelId}
              disabled={models.length <= 1}
              onChange={(event) => onModelChange(event.target.value)}
            >
              {models.map((model) => (
                <option key={model.id} value={model.id}>
                  {model.label}
                </option>
              ))}
            </select>
            {models.length <= 1 && (
              <small>{t("project.regeneration.singleModelNote")}</small>
            )}
          </label>
          <label className="dialogField">
            <span>{t("project.regeneration.outputLanguage")}</span>
            <select
              value={outputLanguage}
              onChange={(event) => onLanguageChange(event.target.value)}
            >
              {LANGUAGE_CODES.map((language) => (
                <option key={language} value={language}>
                  {displayLanguageName(language)}
                </option>
              ))}
            </select>
          </label>
        </>
      ) : (
        <div className="notice error">
          {t("project.regeneration.noCompatibleProvider")}
        </div>
      )}
      <div className="notice regenerationNotice">
        <Icon name="shield" size={17}/>
        <span>{t("project.regeneration.versionNotice")}</span>
      </div>
    </div>
  );
}

function SegmentInsightsPanel({
  artifacts = [],
  selectedVersionByScope,
  onSelect,
  onRegenerate,
  timeline,
}: {
  artifacts?: AnalysisArtifact[];
  selectedVersionByScope: Record<string, string>;
  onSelect: (artifact: AnalysisArtifact) => void;
  onRegenerate: (type: string, artifact: AnalysisArtifact) => void;
  timeline: TimelineSegment[];
}) {
  const scopes = groupArtifactsBySelectionScope(artifacts)
    .map((scopeArtifacts) => usableArtifacts(scopeArtifacts))
    .filter((scopeArtifacts) => scopeArtifacts.length > 0)
    .sort((left, right) => {
      const leftTime = segmentInsightStart(left.at(-1), timeline);
      const rightTime = segmentInsightStart(right.at(-1), timeline);
      return leftTime - rightTime;
    });
  if (!scopes.length) return null;
  return (
    <details className="segmentInsightsPanel">
      <summary>
        <span>
          <strong>{t("project.segmentInsights")}</strong>
          <small>{t("project.segmentInsightsDescription")}</small>
        </span>
        <span className="countBadge">{scopes.length}</span>
      </summary>
      <div className="segmentInsightList">
        {scopes.map((scopeArtifacts, index) => {
          const current = selectedUsableArtifact(scopeArtifacts, selectedVersionByScope);
          if (!current) return null;
          return (
            <details className="segmentInsightItem" key={artifactSelectionKey(current)}>
              <summary>
                <span className="segmentInsightTime">
                  {segmentInsightTimeRange(current, timeline)}
                </span>
                <strong>{segmentInsightSummary(current)}</strong>
                <span className="segmentInsightMeta">
                  {t("project.resultVersionsCount", { count: scopeArtifacts.length })}
                </span>
              </summary>
              <div className="segmentInsightExpanded">
                <ArtifactVersionScope
                  type="segment_understanding"
                  artifacts={scopeArtifacts}
                  selectedVersionByScope={selectedVersionByScope}
                  onSelect={onSelect}
                  onRegenerate={onRegenerate}
                  timeline={timeline}
                />
              </div>
            </details>
          );
        })}
      </div>
    </details>
  );
}

function ArtifactVersions({
  type,
  artifacts = [],
  selectedVersionByScope,
  onSelect,
  onRegenerate,
  timeline = [],
}: {
  type: string;
  artifacts?: AnalysisArtifact[];
  selectedVersionByScope: Record<string, string>;
  onSelect: (artifact: AnalysisArtifact) => void;
  onRegenerate: (type: string, artifact: AnalysisArtifact) => void;
  timeline?: TimelineSegment[];
}) {
  const usable = usableArtifacts(artifacts);
  if (!usable.length)
    return <div className="emptyState">{t("project.noArtifacts")}</div>;
  return (
    <div className="artifactStack">
      {groupArtifactsBySelectionScope(usable).map((scopeArtifacts) => (
        <ArtifactVersionScope
          key={artifactSelectionKey(scopeArtifacts[0])}
          type={type}
          artifacts={scopeArtifacts}
          selectedVersionByScope={selectedVersionByScope}
          onSelect={onSelect}
          onRegenerate={onRegenerate}
          timeline={timeline}
        />
      ))}
    </div>
  );
}

function ArtifactVersionScope({
  type,
  artifacts,
  selectedVersionByScope,
  onSelect,
  onRegenerate,
  timeline = [],
}: {
  type: string;
  artifacts: AnalysisArtifact[];
  selectedVersionByScope: Record<string, string>;
  onSelect: (artifact: AnalysisArtifact) => void;
  onRegenerate: (type: string, artifact: AnalysisArtifact) => void;
  timeline?: TimelineSegment[];
}) {
  const ordered = usableArtifacts(artifacts).sort(compareArtifacts);
  if (!ordered.length) return <div className="emptyState">{t("project.noUsableArtifacts")}</div>;
  const selectionKey = artifactSelectionKey(ordered[0]);
  const selected = selectedVersionByScope[selectionKey];
  const current =
    ordered.find((artifact) => artifact.id === selected) ?? ordered[ordered.length - 1];
  return (
    <section className={`artifactSection artifact-${type}`}>
      <header className="artifactDocumentHeader">
        <div className="artifactTitleBlock">
          <h2>{artifactTypeLabel(type)}</h2>
          <p>{t("project.generatedAt", { date: new Date(current.createdAt).toLocaleString() })}</p>
        </div>
        <div className="artifactToolbar">
          <label>
            <span>{t("project.version")}</span>
            <select
              value={current.id}
              onChange={(event) => {
                const artifact = ordered.find((item) => item.id === event.target.value);
                if (artifact) onSelect(artifact);
              }}
            >
              {ordered.map((artifact, index) => (
                <option value={artifact.id} key={artifact.id}>
                  {index + 1} · {new Date(artifact.createdAt).toLocaleString()}
                </option>
              ))}
            </select>
          </label>
          <button type="button" onClick={() => onRegenerate(type, current)}>
            {regenerationActionLabel(type)}
          </button>
        </div>
      </header>
      <article className="artifactDocument">
        <ReadableArtifact type={type} value={current.payload}/>
        <details className="technicalDetails">
          <summary>{t("project.technicalDetails")}</summary>
          <div className="technicalDetailsBody">
            <TechnicalArtifactDetails artifact={current} timeline={timeline}/>
          </div>
        </details>
      </article>
    </section>
  );
}

function ProvenanceDetails({ artifact }: { artifact: AnalysisArtifact }) {
  const fields = [
    [human("providerId"), displayProviderName(artifact.providerId)],
    [human("modelId"), displayModelName(artifact.modelId)],
    [human("promptVersion"), artifact.promptVersion],
    [human("schemaVersion"), artifact.schemaVersion],
    [human("appVersion"), artifact.appVersion],
  ];
  return (
    <section className="provenanceBlock">
      <h3>{t("project.provenance")}</h3>
      <dl className="provenanceGrid">
        {fields.map(([label, value]) => (
          <div key={label}>
            <dt>{label}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

function ReadableArtifact({ type, value }: { type: string; value: unknown }) {
  const record = asRecord(value);
  if (!record) return <StructuredValue value={value}/>;
  if (type === "literal_translation") {
    return (
      <div className="translationDocument">
        <span>{displayLanguageName(record.targetLanguage)}</span>
        <p>{sanitizeTranslationText(String(record.translatedText ?? t("common.none")))}</p>
      </div>
    );
  }
  if (type === "meeting_minutes" && Array.isArray(record.sections)) {
    return (
      <div className="minutesDocument">
        {(record.sections as unknown[]).map((section, index) => {
          const sectionRecord = asRecord(section) ?? {};
          return (
            <section key={index}>
              <h3>{String(sectionRecord.title ?? t("analysis.fields.sections"))}</h3>
              <ReadableValue value={sectionRecord.items}/>
            </section>
          );
        })}
        {record.limitations !== undefined && (
          <ArtifactField label="limitations" value={record.limitations}/>
        )}
      </div>
    );
  }
  const fields = orderedReadableEntries(type, record);
  return (
    <div className={`artifactFieldGrid artifactFieldGrid-${type}`}>
      {fields.map(([key, item]) => (
        <ArtifactField key={key} label={key} value={item}/>
      ))}
    </div>
  );
}

function ArtifactField({ label, value }: { label: string; value: unknown }) {
  const classKey = label.replace(/[^a-zA-Z0-9_-]/g, "-");
  return (
    <section className={`artifactField artifactField-${classKey}`}>
      <h3>{human(label)}</h3>
      <ReadableValue value={value} fieldKey={label}/>
    </section>
  );
}

function ReadableValue({ value, fieldKey }: { value: unknown; fieldKey?: string }): JSX.Element {
  if (typeof value === "boolean") {
    return <p>{t(value ? "common.yes" : "common.no")}</p>;
  }
  if (typeof value === "string") {
    return <p>{displayScalarValue(fieldKey, value)}</p>;
  }
  if (Array.isArray(value)) {
    const readableItems = value.filter(hasReadableContent);
    if (!readableItems.length) return <p className="mutedValue">{t("common.none")}</p>;
    return (
      <ul className="artifactList">
        {readableItems.map((item, index) => (
          <li key={index}>
            {item && typeof item === "object" ? (
              <ReadableValue value={item} fieldKey={fieldKey}/>
            ) : (
              <span>{displayScalarValue(fieldKey, item)}</span>
            )}
          </li>
        ))}
      </ul>
    );
  }
  if (value && typeof value === "object") {
    return <ReadableObject value={value as Record<string, unknown>}/>;
  }
  return <p>{value === null || value === undefined ? t("common.none") : String(value)}</p>;
}

function ReadableObject({ value }: { value: Record<string, unknown> }) {
  const primary =
    typeof value.text === "string" && sanitizeGeneratedText(value.text)
      ? sanitizeGeneratedText(value.text)
      : null;
  const secondary = Object.entries(value).filter(
    ([key, item]) =>
      !["text", ...technicalKeys].includes(key) && hasReadableContent(item),
  );
  if (!primary && secondary.length === 0) {
    return <p className="mutedValue">{t("common.none")}</p>;
  }
  return (
    <div className="artifactObject">
      {primary && <p>{primary}</p>}
      {secondary.length > 0 && (
        <dl>
          {secondary.map(([key, item]) => (
            <div key={key}>
              <dt>{human(key)}</dt>
              <dd><ReadableValue value={item} fieldKey={key}/></dd>
            </div>
          ))}
        </dl>
      )}
    </div>
  );
}

function StructuredValue({ value, label }: { value: unknown; label?: string }): JSX.Element {
  if (Array.isArray(value))
    return (
      <section className="resultGroup">
        {label && <h3>{human(label)}</h3>}
        {value.length ? (
          <ul>
            {value.map((item, index) => (
              <li key={index}>
                {typeof item === "object" ? <StructuredValue value={item}/> : String(item)}
              </li>
            ))}
          </ul>
        ) : (
          <p>{t("common.none")}</p>
        )}
      </section>
    );
  if (value && typeof value === "object")
    return (
      <div className="structuredResult">
        {Object.entries(value as Record<string, unknown>).map(([key, item]) => (
          <StructuredValue key={key} label={key} value={item}/>
        ))}
      </div>
    );
  return (
    <section className="resultGroup">
      {label && <h3>{human(label)}</h3>}
      <p>{String(value ?? t("common.none"))}</p>
    </section>
  );
}

const technicalKeys = [
  "evidenceRefs",
  "sourceArtifactIds",
  "projectId",
  "segmentId",
  "sourceId",
  "startMs",
  "endMs",
  "evidenceType",
] as const;

const artifactFieldOrder: Record<string, string[]> = {
  segment_understanding: [
    "coreMeaning",
    "keyFacts",
    "explicitIntents",
    "inferredIntents",
    "ambiguities",
    "guidance",
  ],
  post_meeting_analysis: [
    "overview",
    "majorTopics",
    "keyFacts",
    "confirmedDecisions",
    "conditions",
    "constraints",
    "unresolvedIssues",
    "recommendedFollowUpActions",
    "uncertaintyNotes",
  ],
  communication_review: ["improvedWording", "observations", "scope"],
  intelligent_comparison_report: [
    "overallAssessment",
    "correctlyCaptured",
    "missedOrIncomplete",
    "newlyDiscovered",
    "correctedInterpretations",
    "guidanceRevisions",
    "conclusionChanges",
    "recommendedFollowUps",
  ],
};

function orderedReadableEntries(
  type: string,
  record: Record<string, unknown>,
): Array<[string, unknown]> {
  const visibleEntries = Object.entries(record).filter(
    ([key, item]) =>
      !technicalKeys.includes(key as (typeof technicalKeys)[number]) &&
      hasReadableContent(item),
  );
  const order = artifactFieldOrder[type] ?? [];
  return visibleEntries.sort((left, right) => {
    const leftIndex = order.indexOf(left[0]);
    const rightIndex = order.indexOf(right[0]);
    if (leftIndex === -1 && rightIndex === -1) return left[0].localeCompare(right[0]);
    if (leftIndex === -1) return 1;
    if (rightIndex === -1) return -1;
    return leftIndex - rightIndex;
  });
}

function TechnicalArtifactDetails({
  artifact,
  timeline = [],
}: {
  artifact: AnalysisArtifact;
  timeline?: TimelineSegment[];
}) {
  const record = asRecord(artifact.payload);
  const evidenceRefs = record && Array.isArray(record.evidenceRefs)
    ? record.evidenceRefs.map(asRecord).filter((value): value is Record<string, unknown> => Boolean(value))
    : [];
  const evidenceGroups = groupEvidenceReferences(evidenceRefs, timeline);
  const referencedSegmentIds = new Set(
    evidenceRefs.flatMap((item) => [item.segmentId, item.segment]).filter((value): value is string => typeof value === "string"),
  );
  const referencedSourceIds = new Set(
    evidenceRefs.flatMap((item) => [item.sourceId, item.source]).filter((value): value is string => typeof value === "string"),
  );
  const otherEntries = record
    ? Object.entries(record).filter(([key, value]) => {
        if (!technicalKeys.includes(key as (typeof technicalKeys)[number])) return false;
        if (key === "evidenceRefs") return false;
        if (key === "segmentId" && typeof value === "string" && referencedSegmentIds.has(value)) return false;
        if (key === "sourceId" && typeof value === "string" && referencedSourceIds.has(value)) return false;
        return hasTechnicalContent(value);
      })
    : [];
  return (
    <>
      <ProvenanceDetails artifact={artifact}/>
      {evidenceGroups.length > 0 && (
        <section className="technicalSection">
          <h3>{t("project.evidenceDetails")}</h3>
          <div className="evidenceReferenceList">
            {evidenceGroups.map((references, index) => (
              <EvidenceReferenceDetails
                key={`${String(references[0]?.segmentId ?? references[0]?.segment ?? "evidence")}-${index}`}
                references={references}
                index={index}
                timeline={timeline}
              />
            ))}
          </div>
        </section>
      )}
      {otherEntries.length > 0 && (
        <section className="technicalSection">
          <h3>{t("project.additionalTechnicalDetails")}</h3>
          <dl className="technicalFieldGrid">
            {otherEntries.map(([key, value]) => (
              <TechnicalField key={key} fieldKey={key} value={value}/>
            ))}
          </dl>
        </section>
      )}
    </>
  );
}

function EvidenceReferenceDetails({
  references,
  index,
  timeline,
}: {
  references: Record<string, unknown>[];
  index: number;
  timeline: TimelineSegment[];
}) {
  const normalized = references.map((reference) => normalizeEvidenceReference(reference, timeline));
  const starts = normalized.map((item) => item.startMs).filter((value): value is number => value !== null);
  const ends = normalized.map((item) => item.endMs).filter((value): value is number => value !== null);
  const startMs = starts.length ? Math.min(...starts) : null;
  const endMs = ends.length ? Math.max(...ends) : null;
  const evidenceTypes = [...new Set(normalized.map((item) => item.evidenceType).filter(Boolean))];
  const confidences = [...new Set(normalized.map((item) => item.confidence).filter(Boolean))];
  const grouped = normalized.length > 1;
  return (
    <article className={`evidenceReferenceCard ${grouped ? "evidenceReferenceGrouped" : ""}`}>
      <header>
        <strong>
          {grouped
            ? t("project.evidenceReferenceGroup", { number: index + 1, count: normalized.length })
            : t("project.evidenceReference", { number: index + 1 })}
        </strong>
        {confidences.length === 1 && (
          <span className="statusPill">{displayScalarValue("confidence", confidences[0])}</span>
        )}
      </header>
      <dl className="technicalFieldGrid">
        {evidenceTypes.length === 1 && (
          <div>
            <dt>{human("evidenceType")}</dt>
            <dd>{displayScalarValue("evidenceType", evidenceTypes[0])}</dd>
          </div>
        )}
        {(startMs !== null || endMs !== null) && (
          <div>
            <dt>{t("project.timeRange")}</dt>
            <dd>
              {formatPreciseTimeRange(startMs, endMs)}
              <small>{formatRawOffsets(startMs, endMs)}</small>
            </dd>
          </div>
        )}
      </dl>
      <div className="evidenceSourceList">
        {grouped && <strong>{t("project.corroboratingSources")}</strong>}
        {normalized.map((item, sourceIndex) => (
          <div className="evidenceSourceItem" key={`${item.segmentId ?? "segment"}-${item.sourceId ?? sourceIndex}`}>
            <span>{item.trackRole ? t(`project.trackRoles.${item.trackRole}`) : t("project.evidenceSource", { number: sourceIndex + 1 })}</span>
            {item.segmentId && (
              <label>
                {human("segmentId")}
                <code title={item.segmentId}>{item.segmentId}</code>
              </label>
            )}
            {item.sourceId && (
              <label>
                {human("sourceId")}
                <code title={item.sourceId}>{item.sourceId}</code>
              </label>
            )}
          </div>
        ))}
      </div>
    </article>
  );
}

function TechnicalField({ fieldKey, value }: { fieldKey: string; value: unknown }) {
  return (
    <div className={fieldKey.endsWith("Id") || fieldKey.endsWith("Ids") ? "technicalIdField" : ""}>
      <dt>{human(fieldKey)}</dt>
      <dd>{renderTechnicalValue(fieldKey, value)}</dd>
    </div>
  );
}

function renderTechnicalValue(fieldKey: string, value: unknown): ReactNode {
  if ((fieldKey === "startMs" || fieldKey === "endMs") && numericValue(value) !== null) {
    const milliseconds = numericValue(value)!;
    return (
      <>
        {formatPreciseTime(milliseconds)}
        <small>{t("project.rawMilliseconds", { value: milliseconds.toLocaleString() })}</small>
      </>
    );
  }
  if (Array.isArray(value)) {
    return value.length ? (
      <ul className="technicalValueList">
        {value.map((item, index) => (
          <li key={index}>
            {typeof item === "string" ? <code>{item}</code> : <StructuredValue value={item}/>}
          </li>
        ))}
      </ul>
    ) : t("common.none");
  }
  if (typeof value === "string") {
    const displayed = displayScalarValue(fieldKey, value);
    return fieldKey.endsWith("Id") ? <code>{displayed}</code> : displayed;
  }
  if (value && typeof value === "object") return <StructuredValue value={value}/>;
  return String(value ?? t("common.none"));
}

function hasTechnicalContent(value: unknown) {
  if (value === null || value === undefined) return false;
  if (typeof value === "string") return Boolean(value.trim());
  if (Array.isArray(value)) return value.length > 0;
  return true;
}

function numericValue(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim() && Number.isFinite(Number(value))) return Number(value);
  return null;
}

function stringValue(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value : null;
}

function compareArtifacts(left: AnalysisArtifact, right: AnalysisArtifact) {
  return left.createdAt.localeCompare(right.createdAt) || left.id.localeCompare(right.id);
}

function isUsableArtifact(artifact: AnalysisArtifact) {
  return artifact.status === "completed" && hasReadableContent(artifact.payload);
}

function usableArtifacts(artifacts: AnalysisArtifact[]) {
  return artifacts.filter(isUsableArtifact).sort(compareArtifacts);
}

function selectedUsableArtifact(
  artifacts: AnalysisArtifact[],
  selectedVersionByScope: Record<string, string>,
) {
  const ordered = usableArtifacts(artifacts);
  const first = ordered[0];
  if (!first) return undefined;
  const selectedId = selectedVersionByScope[artifactSelectionKey(first)];
  return ordered.find((artifact) => artifact.id === selectedId) ?? ordered.at(-1);
}

function segmentInsightSegment(artifact: AnalysisArtifact, timeline: TimelineSegment[]) {
  return timeline.find((segment) => artifact.sourceIds.includes(segment.id));
}

function segmentInsightStart(artifact: AnalysisArtifact | undefined, timeline: TimelineSegment[]) {
  if (!artifact) return Number.MAX_SAFE_INTEGER;
  const segment = segmentInsightSegment(artifact, timeline);
  if (segment) return segment.startMs;
  const record = asRecord(artifact.payload);
  const refs = record && Array.isArray(record.evidenceRefs) ? record.evidenceRefs.map(asRecord).filter(Boolean) : [];
  const starts = refs.map((reference) => numericValue(reference?.startMs ?? reference?.start)).filter((value): value is number => value !== null);
  return starts.length ? Math.min(...starts) : Number.MAX_SAFE_INTEGER;
}

function segmentInsightTimeRange(artifact: AnalysisArtifact, timeline: TimelineSegment[]) {
  const segment = segmentInsightSegment(artifact, timeline);
  if (segment) return `${formatTime(segment.startMs)}–${formatTime(segment.endMs)}`;
  const record = asRecord(artifact.payload);
  const refs = record && Array.isArray(record.evidenceRefs) ? record.evidenceRefs.map(asRecord).filter(Boolean) : [];
  const starts = refs.map((reference) => numericValue(reference?.startMs ?? reference?.start)).filter((value): value is number => value !== null);
  const ends = refs.map((reference) => numericValue(reference?.endMs ?? reference?.end)).filter((value): value is number => value !== null);
  return starts.length || ends.length
    ? formatPreciseTimeRange(starts.length ? Math.min(...starts) : null, ends.length ? Math.max(...ends) : null)
    : t("project.timeNotAvailable");
}

function segmentInsightSummary(artifact: AnalysisArtifact) {
  const record = asRecord(artifact.payload);
  const coreMeaning = record?.coreMeaning;
  return typeof coreMeaning === "string" && coreMeaning.trim()
    ? sanitizeGeneratedText(coreMeaning)
    : t("project.segmentInsightFallback");
}

interface NormalizedEvidenceReference {
  raw: Record<string, unknown>;
  startMs: number | null;
  endMs: number | null;
  segmentId: string | null;
  sourceId: string | null;
  confidence: string | null;
  evidenceType: string | null;
  trackRole: TimelineSegment["trackRole"] | null;
}

function normalizeEvidenceReference(
  reference: Record<string, unknown>,
  timeline: TimelineSegment[],
): NormalizedEvidenceReference {
  const segmentId = stringValue(reference.segmentId ?? reference.segment);
  const segment = segmentId ? timeline.find((item) => item.id === segmentId) : undefined;
  return {
    raw: reference,
    startMs: numericValue(reference.startMs ?? reference.start),
    endMs: numericValue(reference.endMs ?? reference.end),
    segmentId,
    sourceId: stringValue(reference.sourceId ?? reference.source),
    confidence: stringValue(reference.confidence),
    evidenceType: stringValue(reference.evidenceType ?? reference.type),
    trackRole: segment?.trackRole ?? null,
  };
}

function evidenceOverlapRatio(left: NormalizedEvidenceReference, right: NormalizedEvidenceReference) {
  if (left.startMs === null || left.endMs === null || right.startMs === null || right.endMs === null) return 0;
  const intersection = Math.max(0, Math.min(left.endMs, right.endMs) - Math.max(left.startMs, right.startMs));
  const shorter = Math.min(Math.max(1, left.endMs - left.startMs), Math.max(1, right.endMs - right.startMs));
  return intersection / shorter;
}

function canGroupEvidenceReferences(
  left: NormalizedEvidenceReference,
  right: NormalizedEvidenceReference,
) {
  if (!left.sourceId || !right.sourceId || left.sourceId === right.sourceId) return false;
  if (!left.trackRole || !right.trackRole || left.trackRole === right.trackRole) return false;
  if (left.evidenceType && right.evidenceType && left.evidenceType !== right.evidenceType) return false;
  return evidenceOverlapRatio(left, right) >= 0.8;
}

function groupEvidenceReferences(
  references: Record<string, unknown>[],
  timeline: TimelineSegment[],
) {
  const groups: Record<string, unknown>[][] = [];
  for (const reference of references) {
    const normalized = normalizeEvidenceReference(reference, timeline);
    const group = groups.find((items) =>
      items.some((item) => canGroupEvidenceReferences(normalized, normalizeEvidenceReference(item, timeline))),
    );
    if (group) group.push(reference);
    else groups.push([reference]);
  }
  return groups;
}

function formatPreciseTime(milliseconds: number) {
  const safe = Math.max(0, Math.round(milliseconds));
  const hours = Math.floor(safe / 3_600_000);
  const minutes = Math.floor((safe % 3_600_000) / 60_000);
  const seconds = Math.floor((safe % 60_000) / 1_000);
  const ms = safe % 1_000;
  const prefix = hours > 0 ? `${String(hours).padStart(2, "0")}:` : "";
  return `${prefix}${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}.${String(ms).padStart(3, "0")}`;
}

function formatPreciseTimeRange(startMs: number | null, endMs: number | null) {
  if (startMs !== null && endMs !== null) return `${formatPreciseTime(startMs)}–${formatPreciseTime(endMs)}`;
  if (startMs !== null) return t("project.fromTime", { time: formatPreciseTime(startMs) });
  if (endMs !== null) return t("project.untilTime", { time: formatPreciseTime(endMs) });
  return t("common.unknown");
}

function formatRawOffsets(startMs: number | null, endMs: number | null) {
  if (startMs !== null && endMs !== null) {
    return t("project.rawTimeRange", {
      start: startMs.toLocaleString(),
      end: endMs.toLocaleString(),
    });
  }
  const value = startMs ?? endMs;
  return value === null ? "" : t("project.rawMilliseconds", { value: value.toLocaleString() });
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function isRegeneratableArtifactType(type: string): type is RegeneratableArtifactType {
  return [
    "literal_translation",
    "segment_understanding",
    "post_meeting_analysis",
    "communication_review",
    "intelligent_comparison_report",
    "meeting_minutes",
  ].includes(type);
}

function artifactCapability(type: RegeneratableArtifactType): keyof ProviderDefinition["capabilities"] {
  if (type === "literal_translation") return "textTranslation";
  if (type === "segment_understanding") return "segmentUnderstanding";
  if (type === "post_meeting_analysis") return "meetingSynthesis";
  if (type === "communication_review") return "communicationReview";
  if (type === "intelligent_comparison_report") return "comparisonReport";
  return "meetingMinutes";
}

function regenerationSources(
  type: RegeneratableArtifactType,
  artifact: AnalysisArtifact,
  byType: Record<string, AnalysisArtifact[]>,
  selectedVersionByScope: Record<string, string>,
) {
  if (type === "meeting_minutes") {
    const analysis = selectedArtifactForType(
      byType.post_meeting_analysis ?? [],
      selectedVersionByScope,
    );
    const review = selectedArtifactForType(
      byType.communication_review ?? [],
      selectedVersionByScope,
    );
    if (!analysis || !review) return null;
    return {
      sourceArtifactIds: [analysis.id, review.id],
      sourceSegmentIds: [...new Set([...analysis.sourceIds, ...review.sourceIds])],
    };
  }

  const payload = asRecord(artifact.payload);
  const sourceArtifactIds = Array.isArray(payload?.sourceArtifactIds)
    ? payload.sourceArtifactIds.filter(
        (value): value is string => typeof value === "string",
      )
    : [];
  return {
    sourceArtifactIds,
    sourceSegmentIds: artifact.sourceIds,
  };
}

function selectedArtifactForType(
  artifacts: AnalysisArtifact[],
  selectedVersionByScope: Record<string, string>,
) {
  const ordered = usableArtifacts(artifacts);
  if (!ordered.length) return undefined;
  const selectionKey = artifactSelectionKey(ordered[0]);
  const selectedId = selectedVersionByScope[selectionKey];
  return ordered.find((artifact) => artifact.id === selectedId) ?? ordered.at(-1);
}

function jobKindLabel(kind: string) {
  if (kind === "regenerate") return t("project.processingKinds.regenerate");
  if (kind === "realtime_finalize") return t("project.processingKinds.realtimeFinalize");
  if (kind === "upload") return t("project.processingKinds.upload");
  return human(kind);
}

function compatibleRegenerationProviders(
  type: RegeneratableArtifactType,
  providers: ProviderDefinition[],
  statuses: ProviderConfigurationStatus[],
) {
  const capability = artifactCapability(type);
  return providers.flatMap((definition) => {
    const status = statuses.find((item) => item.providerId === definition.id);
    return status?.configured && definition.capabilities[capability]
      ? [{ definition, status }]
      : [];
  });
}

function regenerationModelOptions(
  type: RegeneratableArtifactType,
  artifact: AnalysisArtifact,
  definition: ProviderDefinition,
  status: ProviderConfigurationStatus,
) {
  const capability = artifactCapability(type);
  const assignment = definition.modelAssignments.find(
    (item) => item.capability === capability,
  );
  const configured = assignment
    ? status.configuration[assignment.configurationFieldId]
    : undefined;
  const values: string[] = [];
  if (typeof configured === "string" && configured.trim()) values.push(configured.trim());
  if (definition.id === "mock") {
    values.length = 0;
    values.push(mockModelIdFor(type));
  } else if (definition.id === artifact.providerId && artifact.modelId.trim()) {
    values.push(artifact.modelId.trim());
  }
  return [...new Set(values)].map((id) => ({
    id,
    label: definition.id === "mock"
      ? t(`project.regeneration.mockModels.${type}`)
      : displayModelName(id),
  }));
}

function mockModelIdFor(type: RegeneratableArtifactType) {
  if (type === "literal_translation") return "mock-translation-v1";
  if (type === "segment_understanding") return "mock-understanding-v1";
  if (type === "post_meeting_analysis") return "mock-analysis-v1";
  if (type === "communication_review") return "mock-review-v1";
  if (type === "intelligent_comparison_report") return "mock-comparison-v1";
  return "mock-minutes-v1";
}

function artifactOutputLanguage(artifact: AnalysisArtifact) {
  const payload = asRecord(artifact.payload);
  if (!payload) return null;
  for (const key of ["language", "targetLanguage", "outputLanguage"]) {
    const value = payload[key];
    if (typeof value === "string" && LANGUAGE_CODES.includes(value as (typeof LANGUAGE_CODES)[number])) {
      return value;
    }
  }
  return null;
}

function regenerationDialogTitle(type: RegeneratableArtifactType) {
  return t(`project.regeneration.titles.${type}`);
}

function regenerationActionLabel(type: string) {
  if (!isRegeneratableArtifactType(type)) return t("project.regeneration.submit");
  return t(`project.regeneration.actions.${type}`);
}

function artifactTypeLabel(type: string) {
  if (type === "literal_translation") return t("analysis.literalTranslation");
  if (type === "segment_understanding") return t("analysis.segmentUnderstanding");
  if (type === "post_meeting_analysis") return t("analysis.postMeetingAnalysis");
  if (type === "communication_review") return t("analysis.communicationReview");
  if (type === "intelligent_comparison_report") return t("comparison.title");
  if (type === "meeting_minutes") return t("minutes.title");
  return humanizeFieldKey(type);
}

function hasReadableContent(value: unknown): boolean {
  if (value === null || value === undefined) return false;
  if (typeof value === "string") return value.trim().length > 0;
  if (Array.isArray(value)) return value.some(hasReadableContent);
  if (typeof value === "object") {
    return Object.entries(value as Record<string, unknown>).some(
      ([key, item]) =>
        !technicalKeys.includes(key as (typeof technicalKeys)[number]) &&
        hasReadableContent(item),
    );
  }
  return true;
}

const languageFieldKeys = new Set([
  "language",
  "targetLanguage",
  "sourceLanguage",
  "translationTargetLanguage",
  "analysisOutputLanguage",
  "minutesOutputLanguage",
]);

function isLanguageField(fieldKey?: string) {
  return Boolean(fieldKey && languageFieldKeys.has(fieldKey));
}

function displayMediaStatus(status: string) {
  const key = `project.mediaStatus.${status}`;
  const label = t(key);
  return label === key ? humanizeFieldKey(status) : label;
}

function displayRunStatus(status: string) {
  const commonKey = `common.${status}`;
  const commonLabel = t(commonKey);
  if (commonLabel !== commonKey) return commonLabel;
  const stageKey = `project.jobStages.${status}`;
  const stageLabel = t(stageKey);
  return stageLabel === stageKey ? humanizeFieldKey(status) : stageLabel;
}

function displayScalarValue(fieldKey: string | undefined, value: unknown) {
  if (typeof value !== "string") return String(value ?? t("common.none"));
  const sanitized = sanitizeGeneratedText(value);
  if (isLanguageField(fieldKey)) return displayLanguageName(sanitized);
  const valueKey = `analysis.values.${sanitized}`;
  const translated = t(valueKey);
  if (translated !== valueKey) return translated;
  return /^[a-z0-9]+(?:_[a-z0-9]+)+$/.test(sanitized)
    ? humanizeFieldKey(sanitized)
    : sanitized;
}

function sanitizeGeneratedText(value: string) {
  return value.replace(/^\s*\[local fixture\]\s*/i, "").trimStart();
}

function sanitizeTranslationText(value: string) {
  return sanitizeGeneratedText(value)
    .split(/\r?\n/)
    .map((line) => line.replace(
      /^\s*\[\s*\d+\s*-\s*\d+\s*ms\s*\]\s*\[[^\]\r\n]+\]\s*/i,
      "",
    ))
    .join("\n")
    .trimStart();
}

function displayProviderName(value: string) {
  const normalized = value.trim();
  if (/^openai$/i.test(normalized)) return "OpenAI";
  if (/^mock$/i.test(normalized)) return "MockProvider";
  return humanizeFieldKey(normalized);
}

function displayModelName(value: string) {
  return value.trim().replace(/^gpt(?=[-_.0-9])/i, "GPT");
}

function displayLanguageName(value: unknown) {
  if (typeof value !== "string" || !value.trim()) return t("common.unknown");
  const key = languageLabelKey(value);
  const label = t(key);
  return label === key ? value : label;
}

function human(value: string) {
  const key = `analysis.fields.${value}`;
  const translated = t(key);
  return translated === key ? humanizeFieldKey(value) : translated;
}

function humanizeFieldKey(value: string) {
  const normalized = value
    .replace(/([A-Z]+)([A-Z][a-z])/g, "$1 $2")
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .replace(/[_-]+/g, " ")
    .trim();
  if (!normalized) return value;
  const acronyms = new Set(["ai", "api", "id", "ids", "ms", "url"]);
  return normalized
    .split(/\s+/)
    .map((word, index) => {
      const lower = word.toLowerCase();
      if (acronyms.has(lower)) return lower.toUpperCase();
      return index === 0
        ? lower.charAt(0).toUpperCase() + lower.slice(1)
        : lower;
    })
    .join(" ");
}

function Info({
  label,
  value,
  icon,
  numeric = false,
  status,
}: {
  label: string;
  value: string;
  icon: IconName;
  numeric?: boolean;
  status?: MeetingProject["status"];
}) {
  return (
    <div className={`infoTile overviewSummaryTile${numeric ? " overviewSummaryTile-numeric" : ""}`}>
      <span className="overviewSummaryIcon">
        <Icon name={icon} size={18}/>
      </span>
      <span className="overviewSummaryContent">
        <span className="overviewSummaryLabel">{label}</span>
        {status ? (
          <span className={`statusBadge status-${status}`}>{value}</span>
        ) : (
          <strong className="overviewSummaryValue">{value}</strong>
        )}
      </span>
    </div>
  );
}

function originIcon(origin: ProjectDetail["project"]["origin"]): IconName {
  return origin === "realtime_online"
    ? "online"
    : origin === "realtime_in_person"
      ? "inPerson"
      : "upload";
}

function originLabel(origin: ProjectDetail["project"]["origin"]) {
  return t(
    origin === "realtime_online"
      ? "project.realtimeOnline"
      : origin === "realtime_in_person"
        ? "project.realtimeInPerson"
        : "project.uploadOnly",
  );
}

function formatTime(ms: number) {
  const minutes = Math.floor(ms / 60000);
  const seconds = Math.floor((ms % 60000) / 1000);
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}
