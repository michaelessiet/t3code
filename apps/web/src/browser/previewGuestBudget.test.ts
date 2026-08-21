import { describe, expect, it } from "vite-plus/test";

import {
  MAX_HIDDEN_IDLE_PREVIEW_GUESTS,
  selectLivePreviewGuests,
  type PreviewGuestCandidate,
} from "./previewGuestBudget";

const candidate = (overrides: Partial<PreviewGuestCandidate> & { tabId: string }) => ({
  updatedAt: "2026-08-21T10:00:00.000Z",
  visible: false,
  controller: "none" as const,
  pictureInPicture: false,
  recording: false,
  ...overrides,
});

describe("selectLivePreviewGuests", () => {
  it("always keeps visible, controlled, picture-in-picture, and recording tabs", () => {
    const live = selectLivePreviewGuests(
      [
        candidate({ tabId: "visible", visible: true }),
        candidate({ tabId: "agent", controller: "agent" }),
        candidate({ tabId: "human", controller: "human" }),
        candidate({ tabId: "pip", pictureInPicture: true }),
        candidate({ tabId: "recording", recording: true }),
      ],
      0,
    );
    expect(live).toEqual(new Set(["visible", "agent", "human", "pip", "recording"]));
  });

  it("keeps only the most recently updated hidden idle tabs within the budget", () => {
    const live = selectLivePreviewGuests(
      [
        candidate({ tabId: "oldest", updatedAt: "2026-08-21T09:00:00.000Z" }),
        candidate({ tabId: "newer", updatedAt: "2026-08-21T11:00:00.000Z" }),
        candidate({ tabId: "newest", updatedAt: "2026-08-21T12:00:00.000Z" }),
      ],
      2,
    );
    expect(live).toEqual(new Set(["newer", "newest"]));
  });

  it("does not count protected tabs against the hidden-idle budget", () => {
    const live = selectLivePreviewGuests(
      [
        candidate({ tabId: "visible-a", visible: true }),
        candidate({ tabId: "visible-b", visible: true }),
        candidate({ tabId: "hidden", updatedAt: "2026-08-21T09:00:00.000Z" }),
      ],
      1,
    );
    expect(live).toEqual(new Set(["visible-a", "visible-b", "hidden"]));
  });

  it("keeps every hidden idle tab while within the default budget", () => {
    const tabs = Array.from({ length: MAX_HIDDEN_IDLE_PREVIEW_GUESTS }, (_, index) =>
      candidate({ tabId: `tab-${index}` }),
    );
    expect(selectLivePreviewGuests(tabs).size).toBe(MAX_HIDDEN_IDLE_PREVIEW_GUESTS);
  });

  it("clamps a negative budget to zero hidden idle tabs", () => {
    expect(selectLivePreviewGuests([candidate({ tabId: "hidden" })], -1)).toEqual(new Set());
  });
});
