import { lazy, Suspense, useRef } from "react";

import { useContentLayout } from "../../lib/layout";
import { useOverlayState } from "../../lib/overlay";
import { activeTab, useStore } from "../../lib/store";
import { internalPageOf, isInternalUrl } from "../../lib/urls";
import { inTauri } from "../../lib/window";
import { NewTab } from "../../pages/NewTab";
import { Spinner } from "../ui/controls";
import { DevPreviewNotice } from "./DevPreviewNotice";
import { ErrorBoundary } from "./ErrorBoundary";

// Internal pages other than the new tab page load on demand to keep startup fast.
const Settings = lazy(() => import("../../pages/Settings"));
const History = lazy(() => import("../../pages/History"));
const Bookmarks = lazy(() => import("../../pages/Bookmarks"));
const Downloads = lazy(() => import("../../pages/Downloads"));
const Performance = lazy(() => import("../../pages/Performance"));
const Gaming = lazy(() => import("../../pages/Gaming"));

function InternalPage({ page }: { page: string }) {
  switch (page) {
    case "settings":
      return <Settings />;
    case "history":
      return <History />;
    case "bookmarks":
      return <Bookmarks />;
    case "downloads":
      return <Downloads />;
    case "performance":
      return <Performance />;
    case "gaming":
      return <Gaming />;
    default:
      return <NewTab />;
  }
}

/**
 * The page area. For web tabs it is an empty slot: the core places the native
 * webview over exactly this rectangle. For browser:// pages it renders the page.
 * Outside the native app (plain `npm run dev`) the slot shows a development-preview notice.
 */
export function ContentHost() {
  const tab = useStore(activeTab);
  const overlay = useOverlayState();
  const ref = useRef<HTMLElement>(null);
  useContentLayout(ref, true);

  const internal = !!tab && isInternalUrl(tab.url);
  const page = tab ? internalPageOf(tab.url) : null;

  return (
    <main ref={ref} className={`content${internal ? " is-internal" : " is-web"}`} id="content">
      {internal ? (
        <ErrorBoundary key={tab?.id}>
          <Suspense fallback={<div className="page-loading"><Spinner size={22} /></div>}>
            <div className="page-scroll" key={tab?.url}>
              <InternalPage page={page ?? "newtab"} />
            </div>
          </Suspense>
        </ErrorBoundary>
      ) : (
        <>
          {!inTauri && tab && <DevPreviewNotice url={tab.url} />}
          {overlay.open && overlay.snapshot ? <img className="overlay-snapshot" src={overlay.snapshot} alt="" draggable={false} /> : null}
          {tab?.loading && tab.lifecycle !== "live" && <div className="web-loading"><Spinner size={22} /></div>}
        </>
      )}
    </main>
  );
}
