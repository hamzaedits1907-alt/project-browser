import { useState } from "react";
import type { ReactNode } from "react";

import { Icon } from "../components/ui/Icon";
import { PageHeader } from "../components/ui/Section";
import { readLocal, writeLocal } from "../lib/storage";
import { ui } from "../lib/ui";
import { About } from "./settings/About";
import { Appearance } from "./settings/Appearance";
import { AiSettings } from "./settings/AiSettings";
import { GamingSettings } from "./settings/GamingSettings";
import { ExtensionsSettings } from "./settings/ExtensionsSettings";
import { General } from "./settings/General";
import { ImportSection } from "./settings/ImportSection";
import { Passwords } from "./settings/Passwords";
import { PerformanceSettings } from "./settings/PerformanceSettings";
import { Privacy } from "./settings/Privacy";
import { Profiles } from "./settings/Profiles";
import { ShieldSettings } from "./settings/ShieldSettings";
import { Shortcuts } from "./settings/Shortcuts";
import { TunnelSettings } from "./settings/TunnelSettings";
import { SECTION_META, matchSections, resolveSection } from "./settingsIndex";

const RENDER: Record<string, () => ReactNode> = {
  general: () => <General />,
  appearance: () => <Appearance />,
  performance: () => <PerformanceSettings />,
  gaming: () => <GamingSettings />,
  privacy: () => <Privacy />,
  shield: () => <ShieldSettings />,
  tunnel: () => <TunnelSettings />,
  extensions: () => <ExtensionsSettings />,
  passwords: () => <Passwords />,
  profiles: () => <Profiles />,
  import: () => <ImportSection />,
  ai: () => <AiSettings />,
  shortcuts: () => <Shortcuts />,
  about: () => <About />,
};

export default function Settings() {
  const [id, setId] = useState(() => resolveSection(readLocal<string>("settings.section", "general")));
  const [query, setQuery] = useState("");
  const jump = ui.use((s) => s.settingsJump);
  const [seenJump, setSeenJump] = useState(jump.n);
  // Something asked for a section while this page was already open (sidebar, menu, AI panel): follow it.
  if (jump.n !== seenJump) {
    setSeenJump(jump.n);
    setId(resolveSection(jump.section));
  }
  const section = SECTION_META.find((s) => s.id === id) ?? SECTION_META[0];
  if (!section) return null;
  const visible = matchSections(query);
  const open = (next: string) => {
    setId(next);
    writeLocal("settings.section", next);
  };
  return (
    <div className="page settings-page">
      <PageHeader icon="settings" title="Settings" subtitle="Everything is saved as you change it." />
      <div className="settings-layout">
        <nav className="settings-nav" aria-label="Settings sections">
          <div className="settings-search">
            <Icon name="search" size={15} />
            <input
              type="search"
              aria-label="Search settings"
              placeholder="Search settings"
              value={query}
              spellCheck={false}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                const first = visible[0];
                if (e.key === "Enter" && first) {
                  open(first.id);
                  setQuery("");
                } else if (e.key === "Escape") setQuery("");
              }}
            />
          </div>
          {visible.map((s, i) => (
            <Group key={s.id} first={i === 0 || visible[i - 1]?.group !== s.group} group={s.group}>
            <button
              key={s.id}
              type="button"
              className={`nav-item${s.id === id ? " is-active" : ""}`}
              aria-current={s.id === id ? "page" : undefined}
              onClick={() => open(s.id)}
            >
              <Icon name={s.icon} size={17} />
              {s.label}
            </button>
            </Group>
          ))}
          {visible.length === 0 && <p className="dim small settings-none">Nothing matches “{query}”.</p>}
        </nav>
        <div className="settings-content" key={section.id}>
          <h2 className="section-title">
            <Icon name={section.icon} size={20} />
            {section.label}
          </h2>
          {RENDER[section.id]?.()}
        </div>
      </div>
    </div>
  );
}

function Group({ first, group, children }: { first: boolean; group: string; children: ReactNode }) {
  return (
    <>
      {first && <div className="eyebrow nav-group-label">{group}</div>}
      {children}
    </>
  );
}
