import { useEffect, useId, useRef } from "react";
import type { ReactNode } from "react";
import { Icon } from "./Icon";

interface Props {
  open: boolean;
  title: string;
  description?: string;
  tone?: "default" | "danger" | "irreversible";
  children?: ReactNode;
  actions: ReactNode;
  closeLabel: string;
  onClose: () => void;
  className?: string;
}

const focusableSelector = [
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[href]",
  '[tabindex]:not([tabindex="-1"])',
].join(",");

export function Dialog({
  open,
  title,
  description,
  tone = "default",
  children,
  actions,
  closeLabel,
  onClose,
  className = "",
}: Props) {
  const titleId = useId();
  const descriptionId = useId();
  const closeButtonRef = useRef<HTMLButtonElement>(null);
  const surfaceRef = useRef<HTMLElement>(null);
  const onCloseRef = useRef(onClose);

  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  useEffect(() => {
    if (!open) return;
    const previouslyFocused = document.activeElement as HTMLElement | null;
    const preferred = surfaceRef.current?.querySelector<HTMLElement>(
      ".dialogBody input:not([disabled]), .dialogBody select:not([disabled]), .dialogBody textarea:not([disabled])",
    );
    (preferred ?? closeButtonRef.current)?.focus();

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onCloseRef.current();
        return;
      }
      if (event.key !== "Tab" || !surfaceRef.current) return;
      const focusable = Array.from(
        surfaceRef.current.querySelectorAll<HTMLElement>(focusableSelector),
      ).filter((element) => !element.hasAttribute("disabled"));
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      previouslyFocused?.focus();
    };
  }, [open]);

  if (!open) return null;

  const role = tone === "default" ? "dialog" : "alertdialog";

  return (
    <div
      className="dialogBackdrop"
      onMouseDown={(event) => {
        if (event.currentTarget === event.target) onClose();
      }}
    >
      <section
        ref={surfaceRef}
        className={`dialogSurface dialogSurface-${tone} ${className}`.trim()}
        role={role}
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={description ? descriptionId : undefined}
      >
        <header className="dialogHeader">
          <div>
            <h2 id={titleId}>{title}</h2>
            {description && <p id={descriptionId}>{description}</p>}
          </div>
          <button
            ref={closeButtonRef}
            className="iconButton dialogCloseButton"
            type="button"
            aria-label={closeLabel}
            onClick={onClose}
          >
            <Icon name="close" size={17} />
          </button>
        </header>
        {children && <div className="dialogBody">{children}</div>}
        <footer className="dialogActions">{actions}</footer>
      </section>
    </div>
  );
}
