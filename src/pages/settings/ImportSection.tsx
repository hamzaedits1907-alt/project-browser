import { Button } from "../../components/ui/controls";
import { Card, Notice, SettingRow } from "../../components/ui/Section";
import { writeLocal } from "../../lib/storage";
import { api } from "../../lib/ipc";
import { attempt } from "../../lib/store";
import { openImportWizard } from "../../lib/ui";

export function ImportSection() {
  const openPasswords = () => {
    writeLocal("settings.section", "passwords");
    void attempt(() => api.runCommand({ type: "openSettings" }));
  };
  return (
    <>
      <Card title="Import from another browser" subtitle="Bring your bookmarks, history, saved passwords and more from Google Chrome, Microsoft Edge or Brave.">
        <SettingRow title="Import wizard" description="Reads the other browser's files on this computer. Nothing is uploaded and the other browser is never modified. Safe to run again: nothing is added twice.">
          <Button variant="primary" icon="import" onClick={openImportWizard}>Start import…</Button>
        </SettingRow>
        <SettingRow title="Saved passwords" description="Import a password file you exported from your old browser, or manage the logins you already imported.">
          <Button icon="key" onClick={openPasswords}>Passwords and autofill</Button>
        </SettingRow>
      </Card>
      <Card title="What can be imported">
        <ul className="plain-list">
          <li><strong>Bookmarks</strong>, with their folders, under an “Imported from …” folder in Other bookmarks.</li>
          <li><strong>Browsing history</strong>: up to the 100,000 most recent pages and 250,000 recent visits, with their original dates.</li>
          <li><strong>Default search engine</strong>, when it is one this browser can use.</li>
          <li><strong>Saved passwords</strong>, from a CSV file you export yourself. They are stored encrypted for your Windows account.</li>
          <li><strong>Saved addresses</strong>, stored encrypted and filled only when you ask.</li>
          <li><strong>Open tabs</strong> from the last session, saved as bookmarks (and optionally reopened). Close the other browser first.</li>
        </ul>
        <Notice tone="warn">
          <strong>Cookies and “stay signed in” sessions are not imported.</strong> Browsers encrypt cookies so that only they can open them, and sites tie sign-ins to
          the browser that created them, so a copy would not keep you signed in. This browser does not try to get around that protection. Sign in once; your imported
          passwords fill the login for you. Payment cards and free-form form history are never imported either.
        </Notice>
      </Card>
    </>
  );
}
