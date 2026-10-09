import { Button } from "../../components/ui/controls";
import { Card, Notice, SettingRow } from "../../components/ui/Section";
import { openImportWizard } from "../../lib/ui";

export function ImportSection() {
  return (
    <>
      <Card title="Import from another browser" subtitle="Bring your bookmarks and browsing history from Google Chrome, Microsoft Edge or Brave.">
        <SettingRow title="Import wizard" description="Reads the other browser's files on this computer. Nothing is uploaded and the other browser is never modified.">
          <Button variant="primary" icon="import" onClick={openImportWizard}>Start import…</Button>
        </SettingRow>
      </Card>
      <Card title="What can be imported">
        <ul className="plain-list">
          <li><strong>Bookmarks</strong> — imported into an “Imported from …” folder under Other bookmarks.</li>
          <li><strong>Browsing history</strong> — up to the 20,000 most recent pages. Adds to your history and improves address bar suggestions.</li>
        </ul>
        <Notice tone="warn">
          <strong>Passwords, autofill, cookies and extensions are not imported.</strong> Chrome, Edge and Brave lock saved passwords with
          encryption that only the original browser can open, and this browser does not try to get around that. To move passwords, export
          them from the old browser's password manager (as a CSV file) and import that file into your password manager.
        </Notice>
      </Card>
    </>
  );
}
