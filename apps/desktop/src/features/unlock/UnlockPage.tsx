import { FormEvent, useState } from "react";
import { ResetVaultDialog } from "../../components/ResetVaultDialog";
import { Icon } from "../../components/Icon";
import { api } from "../../shared/api";
import { t } from "../../i18n";

interface Props {
  vaultExists: boolean;
  errorCode: string | null;
  onUnlocked: () => void;
  onError: (code: string | null) => void;
}

export function UnlockPage({ vaultExists, errorCode, onUnlocked, onError }: Props) {
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [showConfirmPassword, setShowConfirmPassword] = useState(false);
  const [accepted, setAccepted] = useState(false);
  const [resetOpen, setResetOpen] = useState(false);
  const canOfferReset = vaultExists || errorCode === "ERR_ORPHANED_DATA";

  async function submit(event: FormEvent) {
    event.preventDefault();
    onError(null);
    try {
      if (vaultExists) {
        await api.unlock(password);
      } else {
        if (password !== confirmPassword) {
          onError("passwordMismatch");
          return;
        }
        await api.createVault(password);
      }
      onUnlocked();
    } catch (error) {
      onError(String(error));
    }
  }

  return (
    <main className="unlockShell">
      <section className="unlockIntro" aria-hidden="true">
        <div className="unlockBrand">
          <span className="unlockBrandMark"><Icon name="brand" size={28} /></span>
          <div>
            <strong>{t("common.appName")}</strong>
            <span>{t("common.tagline")}</span>
          </div>
        </div>
        <div className="unlockMessage">
          <span className="unlockShield"><Icon name="shield" size={34} /></span>
          <h2>{t("common.localOnly")}</h2>
          <p>{t("settings.privacyBody")}</p>
        </div>
      </section>

      <section className="unlockPanel">
        <div className="unlockPanelHeader">
          <span className="unlockPanelIcon"><Icon name="lock" size={24} /></span>
          <div>
            <h1>{t(vaultExists ? "unlock.unlockTitle" : "unlock.createTitle")}</h1>
            <p>{t(vaultExists ? "unlock.unlockBody" : "unlock.createBody")}</p>
          </div>
        </div>

        {!api.isNative && <div className="notice">{t("unlock.demoMode")}</div>}

        <form onSubmit={submit} className="formStack unlockForm">
          <label>
            {vaultExists ? t("unlock.unlockPassword") : t("unlock.createPassword")}
            <span className="passwordField">
              <input
                type={showPassword ? "text" : "password"}
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                minLength={8}
                autoFocus
                required
              />
              <button
                type="button"
                className="passwordRevealTextButton"
                onClick={() => setShowPassword((value) => !value)}
                aria-label={t(showPassword ? "unlock.hidePassword" : "unlock.showPassword")}
                aria-pressed={showPassword}
              >
                {t(showPassword ? "unlock.hidePassword" : "unlock.showPassword")}
              </button>
            </span>
          </label>
          {!vaultExists && (
            <>
              <label>
                {t("unlock.confirmPassword")}
                <span className="passwordField">
                  <input
                    type={showConfirmPassword ? "text" : "password"}
                    value={confirmPassword}
                    onChange={(event) => setConfirmPassword(event.target.value)}
                    minLength={8}
                    required
                  />
                  <button
                    type="button"
                    className="passwordRevealTextButton"
                    onClick={() => setShowConfirmPassword((value) => !value)}
                    aria-label={t(showConfirmPassword ? "unlock.hidePassword" : "unlock.showPassword")}
                    aria-pressed={showConfirmPassword}
                  >
                    {t(showConfirmPassword ? "unlock.hidePassword" : "unlock.showPassword")}
                  </button>
                </span>
              </label>
              <label className="checkRow consentRow">
                <input
                  type="checkbox"
                  checked={accepted}
                  onChange={(event) => setAccepted(event.target.checked)}
                  required
                />
                <span>{t("unlock.noRecovery")}</span>
              </label>
            </>
          )}
          {errorCode && (
            <div className="notice error" role="alert">
              {t(errorCode === "passwordMismatch" ? "unlock.passwordMismatch" : `errors.${errorCode}`)}
            </div>
          )}
          <button className="primaryButton unlockAction" disabled={!vaultExists && !accepted}>
            <Icon name="lock" size={17} />
            {vaultExists ? t("unlock.unlockAction") : t("unlock.setupAction")}
          </button>
        </form>

        {canOfferReset && (
          <div className="resetArea">
            <button
              className="textDangerButton"
              onClick={() => setResetOpen(true)}
            >
              {t("unlock.resetVault")}
            </button>
          </div>
        )}
      </section>

      <ResetVaultDialog
        open={resetOpen}
        onClose={() => setResetOpen(false)}
        onError={(code) => onError(code)}
      />
    </main>
  );
}
