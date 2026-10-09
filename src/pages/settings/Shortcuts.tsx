import { Kbd } from "../../components/ui/controls";
import { Card } from "../../components/ui/Section";
import { SHORTCUT_GROUPS } from "../../lib/shortcuts";

export function Shortcuts() {
  return (
    <>
      {SHORTCUT_GROUPS.map((g) => (
        <Card key={g.title} title={g.title}>
          <ul className="shortcut-list">
            {g.items.map((it) => (
              <li key={it.label}>
                <span>{it.label}</span>
                <Kbd keys={it.keys} />
              </li>
            ))}
          </ul>
        </Card>
      ))}
    </>
  );
}
