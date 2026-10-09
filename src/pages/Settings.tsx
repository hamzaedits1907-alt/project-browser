import { useState } from "react";
import type { ReactNode } from "react";

import { Icon } from "../components/ui/Icon";
import type { IconName } from "../components/ui/Icon";
import { PageHeader } from "../components/ui/Section";
import { readLocal, writeLocal } from "../lib/storage";
import { About } from "./settings/About";
import { Appearance } from "./settings/Appearance";
import { AiSettings } from "./settings/AiSettings";
import { GamingSettings } from "./settings/GamingSettings";
import { General } from "./settings/General";
import { ImportSection } from "./settings/ImportSection";
import { PerformanceSettings } from "./settings/PerformanceSettings";
import { Privacy } from "./settings/Privacy";
import { Profiles } from "./settings/Profiles";
import { Shortcuts } from "./settings/Shortcuts";

interface Section {
  id: string;
  label: string;
  icon: IconName;
  render: () => ReactNode;
}

const SECTIONS: Section[] = [
  { id: "general", label: "General", icon: "settings", render: () => <General /> },
  { id: "appearance", label: "Appearance", icon: "eye", render: () => <Appearance /> },
  { id: "performance", label: "Performance", icon: "gauge", render: () => <PerformanceSettings /> },
  { id: "gaming", label: "Gaming Mode", icon: "gamepad", render: () => <GamingSettings /> },
  { id: "privacy", label: "Privacy & security", icon: "shield", render: () => <Privacy /> },
  { id: "profiles", label: "Profiles", icon: "user", render: () => <Profiles /> },
  { id: "import", label: "Import data", icon: "import", render: () => <ImportSection /> },
  { id: "ai", label: "AI assistant", icon: "sparkle", render: () => <AiSettings /> },
  { id: "shortcuts", label: "Shortcuts", icon: "bolt", render: () => <Shortcuts /> },
  { id: "about", label: "About", icon: "info", render: () => <About /> },
];

export default function Settings() {
  const [id, setId] = useState(() => {
    const saved = readLocal<string>("settings.section", "general");
    return SECTIONS.some((s) => s.id === saved) ? saved : "general";
  });
  const section = SECTIONS.find((s) => s.id === id) ?? SECTIONS[0];
  if (!section) return null;
  return (
    <div className="page settings-page">
      <PageHeader icon="settings" title="Settings" />
      <div className="settings-layout">
        <nav className="settings-nav" aria-label="Settings sections">
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              type="button"
              className={`nav-item${s.id === id ? " is-active" : ""}`}
              aria-current={s.id === id ? "page" : undefined}
              onClick={() => {
                setId(s.id);
                writeLocal("settings.section", s.id);
              }}
            >
              <Icon name={s.icon} size={17} />
              {s.label}
            </button>
          ))}
        </nav>
        <div className="settings-content" key={section.id}>
          <h2 className="section-title">{section.label}</h2>
          {section.render()}
        </div>
      </div>
    </div>
  );
}
