import { Icon } from "../ui/Icon";

/**
 * Shown in the page area for web tabs when the shell runs in an ordinary browser
 * (`npm run dev`) instead of the native app. There is no web engine in that mode:
 * pages are drawn by WebView2, which only exists inside the Tauri window.
 * Nothing here pretends to render the page.
 */
export function DevPreviewNotice({ url }: { url: string }) {
  return (
    <div className="dev-preview" role="status">
      <Icon name="warning" size={28} />
      <h2>Development preview: no web engine here</h2>
      <p>
        You are looking at the browser interface in an ordinary browser tab (<code>npm run dev</code>), against a mock core.
        Websites are rendered by Microsoft WebView2 inside the native desktop app, so this area stays empty.
      </p>
      <p className="dev-preview-url" data-selectable>{url}</p>
      <p className="dim small">
        To see real pages, run the native app (<code>npm run app:dev</code>) or install the build from the GitHub Actions
        “Windows build” artifact.
      </p>
    </div>
  );
}
