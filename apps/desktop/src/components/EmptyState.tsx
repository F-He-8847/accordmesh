import type { ReactNode } from "react";
import { Icon, type IconName } from "./Icon";

interface Props {
  icon: IconName;
  title: string;
  description: string;
  action?: ReactNode;
  compact?: boolean;
}

export function EmptyState({
  icon,
  title,
  description,
  action,
  compact = false,
}: Props) {
  return (
    <div className={`emptyState ${compact ? "emptyStateCompact" : ""}`}>
      <span className="emptyStateIcon"><Icon name={icon} size={compact ? 22 : 28}/></span>
      <strong>{title}</strong>
      <p>{description}</p>
      {action && <div className="emptyStateAction">{action}</div>}
    </div>
  );
}
