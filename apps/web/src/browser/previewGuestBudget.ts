/**
 * Live-guest budget for desktop preview webviews.
 *
 * Every mounted <webview> keeps a full guest renderer process resident
 * (~90MB+ each for real pages) plus GPU compositor surfaces, even while the
 * tab sits offscreen at HIDDEN_BROWSER_WEBVIEW_OFFSET. Previews accumulate
 * across threads with no cap, so hidden idle tabs dominate the app's memory
 * footprint once a few threads have opened one.
 *
 * Visible tabs and tabs something is actively driving (human/agent
 * controller, picture-in-picture, recording) always stay mounted — agent
 * automation runs against background tabs and must not lose its guest. Of
 * the remaining hidden idle tabs, only the most recently updated stay live.
 * An evicted tab keeps its server-side preview session; when it is shown
 * again the webview remounts and restores from the session's last URL.
 */

export interface PreviewGuestCandidate {
  readonly tabId: string;
  /** ISO timestamp of the last session update (navigation, automation). */
  readonly updatedAt: string;
  readonly visible: boolean;
  readonly controller: "human" | "agent" | "none";
  readonly pictureInPicture: boolean;
  readonly recording: boolean;
}

export const MAX_HIDDEN_IDLE_PREVIEW_GUESTS = 3;

export function selectLivePreviewGuests(
  candidates: ReadonlyArray<PreviewGuestCandidate>,
  maxHiddenIdle: number = MAX_HIDDEN_IDLE_PREVIEW_GUESTS,
): ReadonlySet<string> {
  const live = new Set<string>();
  const hiddenIdle: PreviewGuestCandidate[] = [];
  for (const candidate of candidates) {
    const protectedGuest =
      candidate.visible ||
      candidate.controller !== "none" ||
      candidate.pictureInPicture ||
      candidate.recording;
    if (protectedGuest) {
      live.add(candidate.tabId);
    } else {
      hiddenIdle.push(candidate);
    }
  }
  const budget = Math.max(0, maxHiddenIdle);
  for (const candidate of hiddenIdle
    .toSorted((a, b) => b.updatedAt.localeCompare(a.updatedAt))
    .slice(0, budget)) {
    live.add(candidate.tabId);
  }
  return live;
}
