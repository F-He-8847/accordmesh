import { useMemo, useState } from "react";
import { Dialog } from "../../components/Dialog";
import { Icon } from "../../components/Icon";
import { EmptyState } from "../../components/EmptyState";
import { api } from "../../shared/api";
import type { MeetingProject, ProjectOrigin, ProjectStatus } from "../../shared/types";
import { t } from "../../i18n";
import { canDeleteProject, deleteGuardKey } from "./projectDeletion";
import {
  PROJECT_TITLE_MAX_CHARS,
  projectTitleLength,
} from "../../shared/projectTitle";

interface Props {
  projects: MeetingProject[];
  onOpen: (projectId: string) => void;
  onRefresh: () => Promise<void>;
  onError: (code: string | null) => void;
}

export function LibraryPage({ projects, onOpen, onRefresh, onError }: Props) {
  const [query, setQuery] = useState("");
  const [origin, setOrigin] = useState<ProjectOrigin | "all">("all");
  const [status, setStatus] = useState<ProjectStatus | "all">("all");
  const [sort, setSort] = useState<"newest" | "oldest">("newest");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [titleDraft, setTitleDraft] = useState("");
  const [confirmingDeleteId, setConfirmingDeleteId] = useState<string | null>(null);
  const [actionBusy, setActionBusy] = useState(false);

  const visible = useMemo(() => {
    return projects
      .filter((project) => project.title.toLowerCase().includes(query.toLowerCase()))
      .filter((project) => origin === "all" || project.origin === origin)
      .filter((project) => status === "all" || project.status === status)
      .sort((a, b) =>
        sort === "newest"
          ? b.createdAt.localeCompare(a.createdAt)
          : a.createdAt.localeCompare(b.createdAt),
      );
  }, [origin, projects, query, sort, status]);

  const titleDraftLength = projectTitleLength(titleDraft);
  const titleDraftOverLimit = titleDraftLength > PROJECT_TITLE_MAX_CHARS;
  const editingProject = projects.find((project) => project.id === editingId) ?? null;
  const deletingProject =
    projects.find((project) => project.id === confirmingDeleteId) ?? null;

  async function rename(project: MeetingProject) {
    const title = titleDraft.trim();
    if (titleDraftOverLimit) {
      onError("ERR_TITLE_TOO_LONG");
      return;
    }
    if (!title || title === project.title || actionBusy) {
      setEditingId(null);
      return;
    }
    setActionBusy(true);
    try {
      await api.renameProject(project.id, title);
      await onRefresh();
      setEditingId(null);
      onError(null);
    } catch (error) {
      onError(String(error));
    } finally {
      setActionBusy(false);
    }
  }

  async function remove(project: MeetingProject) {
    if (!canDeleteProject(project) || actionBusy) return;
    setActionBusy(true);
    try {
      await api.deleteProject(project.id);
      await onRefresh();
      setConfirmingDeleteId(null);
      onError(null);
    } catch (error) {
      onError(String(error));
    } finally {
      setActionBusy(false);
    }
  }

  return (
    <section className="libraryPage">
      <div className="pageHeader libraryHeader">
        <div>
          <span className="pageEyebrow">{t("common.localOnly")}</span>
          <h1>{t("library.title")}</h1>
          <p>{t("library.workspaceDescription")}</p>
        </div>
        <div className="librarySummary" aria-label={t("accessibility.projectList")}>
          <strong>{projects.length}</strong>
          <span>
            {t(projects.length === 1 ? "library.projectCountSingle" : "library.projectCountPlural")}
          </span>
        </div>
      </div>

      <div className="libraryToolbar">
        <label className="searchField">
          <Icon name="search" size={17} />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("library.searchPlaceholder")}
          />
        </label>
        <label className="compactSelect">
          <Icon name="filter" size={16} />
          <select
            value={origin}
            aria-label={t("project.origin")}
            onChange={(event) => setOrigin(event.target.value as ProjectOrigin | "all")}
          >
            <option value="all">{t("common.all")}</option>
            <option value="realtime_online">{t("project.realtimeOnline")}</option>
            <option value="realtime_in_person">{t("project.realtimeInPerson")}</option>
            <option value="upload_only">{t("project.uploadOnly")}</option>
          </select>
        </label>
        <label className="compactSelect">
          <Icon name="filter" size={16} />
          <select
            value={status}
            aria-label={t("common.status")}
            onChange={(event) => setStatus(event.target.value as ProjectStatus | "all")}
          >
            <option value="all">{t("common.all")}</option>
            <option value="active">{t("common.active")}</option>
            <option value="completed">{t("common.completed")}</option>
            <option value="processing">{t("common.processing")}</option>
            <option value="failed">{t("common.failed")}</option>
          </select>
        </label>
        <label className="compactSelect">
          <Icon name="sort" size={16} />
          <select
            value={sort}
            aria-label={t("common.date")}
            onChange={(event) => setSort(event.target.value as "newest" | "oldest")}
          >
            <option value="newest">{t("library.sortNewest")}</option>
            <option value="oldest">{t("library.sortOldest")}</option>
          </select>
        </label>
      </div>

      {visible.length === 0 ? (
        <EmptyState
          icon="library"
          title={t("library.empty")}
          description={query ? t("library.noSearchResults") : t("library.emptyHint")}
        />
      ) : (
        <div className="projectGrid" aria-label={t("accessibility.projectList")}>
          {visible.map((project) => {
            const guardKey = deleteGuardKey(project);
            const deletable = guardKey === null;
            return (
              <article className="projectCard" data-status={project.status} key={project.id}>
                <div className="projectCardTopline">
                  <span className={`projectOriginIcon origin-${project.origin}`}>
                    <Icon name={originIcon(project.origin)} size={18} />
                  </span>
                  <span className={`statusBadge status-${project.status}`}>
                    {statusLabel(project.status)}
                  </span>
                </div>

                <div className="projectCardContent">
                  <button className="projectTitleButton" onClick={() => onOpen(project.id)}>
                    <span title={project.title}>{project.title}</span>
                    <Icon name="open" size={15} />
                  </button>
                  <p className="projectMetaPrimary">{originLabel(project.origin)}</p>
                  <p className="projectMetaSecondary">
                    {t("library.createdTime", { time: formatCreatedAt(project.createdAt) })}
                  </p>
                </div>

                <div className="projectFeatureRow">
                  {project.mediaAssetIds.length > 0 && (
                    <span><Icon name="media" size={14} />{t("library.hasMedia")}</span>
                  )}
                  {project.hasComparison && (
                    <span><Icon name="comparison" size={14} />{t("library.hasComparison")}</span>
                  )}
                  {project.hasMinutes && (
                    <span><Icon name="minutes" size={14} />{t("library.hasMinutes")}</span>
                  )}
                </div>

                <div className="projectCardActions">
                  <button className="primaryButton" onClick={() => onOpen(project.id)}>
                    <Icon name="open" size={16} />
                    {t("common.open")}
                  </button>
                  <button
                    className="iconButton"
                    title={t("common.rename")}
                    aria-label={t("common.rename")}
                    onClick={() => {
                      setTitleDraft(project.title);
                      setEditingId(project.id);
                    }}
                  >
                    <Icon name="rename" size={17} />
                  </button>
                  <button
                    className="iconButton dangerButton"
                    disabled={!deletable}
                    title={guardKey ? t(guardKey) : t("common.delete")}
                    aria-label={t("common.delete")}
                    onClick={() => setConfirmingDeleteId(project.id)}
                  >
                    <Icon name="delete" size={17} />
                  </button>
                </div>

                {guardKey && <small className="fieldHelp">{t(guardKey)}</small>}
              </article>
            );
          })}
        </div>
      )}

      <Dialog
        open={Boolean(editingProject)}
        title={t("library.renameDialogTitle")}
        description={t("library.renameDialogDescription")}
        closeLabel={t("common.close")}
        onClose={() => {
          if (!actionBusy) setEditingId(null);
        }}
        actions={
          <>
            <button type="button" disabled={actionBusy} onClick={() => setEditingId(null)}>
              {t("common.cancel")}
            </button>
            <button
              type="button"
              className="primaryButton"
              disabled={!titleDraft.trim() || titleDraftOverLimit || actionBusy}
              onClick={() => editingProject && void rename(editingProject)}
            >
              {actionBusy ? t("common.saving") : t("common.save")}
            </button>
          </>
        }
      >
        <label className="dialogField">
          <span>{t("library.renamePlaceholder")}</span>
          <input
            value={titleDraft}
            aria-invalid={titleDraftOverLimit}
            autoFocus
            onChange={(event) => setTitleDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && editingProject && !titleDraftOverLimit) void rename(editingProject);
            }}
          />
          <small className={`characterCount ${titleDraftOverLimit ? "isOverLimit" : ""}`}>
            {t("library.titleCharacterCount", {
              count: titleDraftLength,
              max: PROJECT_TITLE_MAX_CHARS,
            })}
          </small>
          {titleDraftOverLimit && <small className="fieldError">{t("errors.ERR_TITLE_TOO_LONG")}</small>}
        </label>
      </Dialog>

      <Dialog
        open={Boolean(deletingProject && canDeleteProject(deletingProject))}
        title={t("library.deleteDialogTitle")}
        description={deletingProject ? t("library.deleteDialogDescription") : undefined}
        tone="irreversible"
        closeLabel={t("common.close")}
        onClose={() => {
          if (!actionBusy) setConfirmingDeleteId(null);
        }}
        actions={
          <>
            <button type="button" disabled={actionBusy} onClick={() => setConfirmingDeleteId(null)}>
              {t("common.cancel")}
            </button>
            <button
              type="button"
              className="dangerButton"
              disabled={actionBusy}
              onClick={() => deletingProject && void remove(deletingProject)}
            >
              {actionBusy ? t("common.processing") : t("common.delete")}
            </button>
          </>
        }
      >
        {deletingProject && (
          <p className="dialogProjectName" title={deletingProject.title}>
            {deletingProject.title}
          </p>
        )}
      </Dialog>
    </section>
  );
}

function formatCreatedAt(value: string) {
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

function originIcon(origin: ProjectOrigin) {
  if (origin === "realtime_online") return "online" as const;
  if (origin === "realtime_in_person") return "inPerson" as const;
  return "upload" as const;
}

function originLabel(origin: ProjectOrigin) {
  if (origin === "realtime_online") return t("project.realtimeOnline");
  if (origin === "realtime_in_person") return t("project.realtimeInPerson");
  return t("project.uploadOnly");
}

function statusLabel(status: ProjectStatus) {
  return t(`common.${status}`);
}
