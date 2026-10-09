import { useState } from "react";

import { DownloadRow } from "../components/chrome/DownloadRow";
import { Button } from "../components/ui/controls";
import { Icon } from "../components/ui/Icon";
import { EmptyState, Notice, PageHeader } from "../components/ui/Section";
import { groupByDay } from "../lib/format";
import { api } from "../lib/ipc";
import { attempt, useStore } from "../lib/store";

export default function Downloads() {
  const downloads = useStore((s) => s.downloads);
  const gaming = useStore((s) => s.settings.gamingMode);
  const holdDownloads = useStore((s) => s.settings.gamingPauseDownloads);
  const [query, setQuery] = useState("");

  const q = query.trim().toLowerCase();
  const list = q ? downloads.filter((d) => d.filename.toLowerCase().includes(q) || d.url.toLowerCase().includes(q)) : downloads;
  const active = list.filter((d) => d.state === "inprogress" || d.state === "paused");
  const finished = list.filter((d) => d.state !== "inprogress" && d.state !== "paused");
  const groups = groupByDay(finished, (d) => d.endedAt ?? d.startedAt);

  return (
    <div className="page">
      <PageHeader
        icon="download"
        title="Downloads"
        actions={<Button icon="broom" disabled={finished.length === 0} onClick={() => void attempt(() => api.downloadsClear())}>Clear finished</Button>}
      />
      {gaming && holdDownloads && (
        <Notice tone="info"><Icon name="bolt" size={14} /> Gaming Mode is holding downloads. They continue when you turn it off.</Notice>
      )}
      <div className="page-toolbar">
        <label className="search-field">
          <Icon name="search" size={16} />
          <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search downloads" aria-label="Search downloads" spellCheck={false} />
        </label>
      </div>

      {downloads.length === 0 && <EmptyState icon="download" title="No downloads yet">Files you download appear here, with progress and quick access to the folder.</EmptyState>}
      {downloads.length > 0 && list.length === 0 && <EmptyState icon="search" title="No matching downloads" />}

      {active.length > 0 && (
        <section className="day-group">
          <h2 className="day-label">In progress</h2>
          <div className="list">{active.map((d) => <DownloadRow key={d.id} d={d} />)}</div>
        </section>
      )}
      {groups.map((g) => (
        <section key={g.label} className="day-group">
          <h2 className="day-label">{g.label}</h2>
          <div className="list">{g.items.map((d) => <DownloadRow key={d.id} d={d} />)}</div>
        </section>
      ))}
    </div>
  );
}
