import { useEffect, useRef, useState } from "react";
import { t } from "../i18n";
import { api } from "../shared/api";
import type { ResetVaultStatus } from "../shared/types";
import { Dialog } from "./Dialog";
import { Icon } from "./Icon";

interface Props {
  open: boolean;
  onClose: () => void;
  onError: (code: string) => void;
}

const emptyStatus: ResetVaultStatus = {
  activeRealtimeSessions: 0,
  cleanupPendingSessions: 0,
  activeJobs: 0,
  operationsInFlight: 0,
  resetInProgress: false,
  recoveryRequired: false,
  canStart: false,
  activeWorkBlocksReset: false,
};

export function ResetVaultDialog({ open, onClose, onError }: Props) {
  const [confirmation, setConfirmation] = useState("");
  const [acknowledged, setAcknowledged] = useState(false);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(false);
  const [status, setStatus] = useState<ResetVaultStatus>(emptyStatus);
  const onErrorRef = useRef(onError);

  useEffect(() => {
    onErrorRef.current = onError;
  }, [onError]);

  useEffect(() => {
    if (!open) return;
    let active = true;
    setConfirmation("");
    setAcknowledged(false);
    setBusy(false);
    setLoading(true);
    void api
      .resetVaultStatus()
      .then((value) => {
        if (active) setStatus(value);
      })
      .catch((error) => {
        if (active) {
          setStatus(emptyStatus);
          onErrorRef.current(String(error));
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [open]);

  const canSubmit =
    !loading &&
    !busy &&
    status.canStart &&
    acknowledged &&
    confirmation === "RESET";

  async function performReset() {
    if (!canSubmit) return;
    setBusy(true);
    try {
      await api.resetVault(confirmation);
      window.location.reload();
    } catch (error) {
      onErrorRef.current(String(error));
      setBusy(false);
      try {
        setStatus(await api.resetVaultStatus());
      } catch {
        setStatus(emptyStatus);
      }
    }
  }

  return (
    <Dialog
      open={open}
      title={t("unlock.resetVault")}
      description={t("unlock.resetWarning")}
      tone="irreversible"
      closeLabel={t("common.close")}
      onClose={() => {
        if (!busy) onClose();
      }}
      actions={
        <>
          <button type="button" disabled={busy} onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button
            type="button"
            className="dangerButton"
            disabled={!canSubmit}
            onClick={() => void performReset()}
          >
            {busy ? t("unlock.resetInProgress") : t("unlock.resetAction")}
          </button>
        </>
      }
    >
      <div className="resetVaultDialog">
        <section className="resetImpactCard" aria-label={t("unlock.resetImpactTitle")}>
          <span className="resetImpactIcon" aria-hidden="true">
            <Icon name="delete" size={18} />
          </span>
          <div>
            <strong>{t("unlock.resetImpactTitle")}</strong>
            <ul>
              <li>{t("unlock.resetImpactProjects")}</li>
              <li>{t("unlock.resetImpactCredentials")}</li>
              <li>{t("unlock.resetImpactSettings")}</li>
              <li>{t("unlock.resetImpactExports")}</li>
            </ul>
          </div>
        </section>

        {loading && (
          <div className="notice" role="status">
            {t("unlock.resetChecking")}
          </div>
        )}

        {!loading && status.recoveryRequired && (
          <div className="notice error" role="alert">
            {t("unlock.resetRecoveryRequired")}
          </div>
        )}

        {!loading && status.resetInProgress && !status.recoveryRequired && (
          <div className="notice" role="status">
            {t("unlock.resetAlreadyInProgress")}
          </div>
        )}

        {!loading && status.operationsInFlight > 0 && (
          <div className="notice error" role="alert">
            {t(
              status.operationsInFlight === 1
                ? "unlock.resetOperationBusySingle"
                : "unlock.resetOperationBusyPlural",
              { count: status.operationsInFlight },
            )}
          </div>
        )}

        {!loading &&
          status.activeWorkBlocksReset &&
          status.activeRealtimeSessions +
            status.cleanupPendingSessions +
            status.activeJobs >
            0 && (
            <div className="notice warning resetWorkNotice" role="status">
              <strong>{t("unlock.resetActiveWorkTitle")}</strong>
              <span>{t("unlock.resetActiveWorkBody")}</span>
              <span>
                {t("unlock.resetActiveMeetings", {
                  count:
                    status.activeRealtimeSessions +
                    status.cleanupPendingSessions,
                })}
              </span>
              <span>
                {t("unlock.resetActiveJobs", { count: status.activeJobs })}
              </span>
            </div>
          )}

        <label className="checkRow resetAcknowledgement">
          <input
            type="checkbox"
            checked={acknowledged}
            disabled={busy}
            onChange={(event) => setAcknowledged(event.target.checked)}
          />
          <span>{t("unlock.resetAcknowledgement")}</span>
        </label>

        <label className="dialogField">
          <span>{t("unlock.resetInstruction")}</span>
          <input
            value={confirmation}
            autoComplete="off"
            spellCheck={false}
            disabled={busy}
            onChange={(event) => setConfirmation(event.target.value)}
            placeholder="RESET"
          />
          <small className="fieldHint">{t("unlock.resetFailClosedHint")}</small>
        </label>
      </div>
    </Dialog>
  );
}
