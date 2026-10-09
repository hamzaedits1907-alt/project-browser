import type { ReactNode } from "react";

import { Icon } from "./Icon";
import type { IconName } from "./Icon";
import { cx } from "./controls";

export function Card({ title, subtitle, actions, children, className }: { title?: string; subtitle?: string; actions?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className={cx("card", className)}>
      {(title || actions) && (
        <header className="card-head">
          <div>
            {title && <h3>{title}</h3>}
            {subtitle && <p className="dim">{subtitle}</p>}
          </div>
          {actions && <div className="card-actions">{actions}</div>}
        </header>
      )}
      {children}
    </section>
  );
}

/** One labelled setting: text on the left, control on the right. */
export function SettingRow({ title, description, children, stacked }: { title: string; description?: ReactNode; children?: ReactNode; stacked?: boolean }) {
  return (
    <div className={cx("setting-row", stacked && "is-stacked")}>
      <div className="setting-text">
        <div className="setting-title">{title}</div>
        {description && <div className="setting-desc">{description}</div>}
      </div>
      {children && <div className="setting-control">{children}</div>}
    </div>
  );
}

export function EmptyState({ icon, title, children }: { icon: IconName; title: string; children?: ReactNode }) {
  return (
    <div className="empty-state">
      <div className="empty-icon">
        <Icon name={icon} size={26} />
      </div>
      <h3>{title}</h3>
      {children && <p className="dim">{children}</p>}
    </div>
  );
}

export function PageHeader({ icon, title, subtitle, actions }: { icon: IconName; title: string; subtitle?: string; actions?: ReactNode }) {
  return (
    <header className="page-header">
      <div className="page-title">
        <span className="page-icon">
          <Icon name={icon} size={20} />
        </span>
        <div>
          <h1>{title}</h1>
          {subtitle && <p className="dim">{subtitle}</p>}
        </div>
      </div>
      {actions && <div className="page-actions">{actions}</div>}
    </header>
  );
}

export function Notice({ tone = "info", children }: { tone?: "info" | "warn" | "bad" | "good"; children: ReactNode }) {
  const icon: IconName = tone === "bad" || tone === "warn" ? "warning" : tone === "good" ? "check" : "info";
  return (
    <div className={cx("notice", `notice-${tone}`)} role={tone === "bad" ? "alert" : "note"}>
      <Icon name={icon} size={16} />
      <div>{children}</div>
    </div>
  );
}
