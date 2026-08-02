import { assert, describe, it } from "vite-plus/test";

import { clampMenuToViewport } from "./menuPosition";

const VIEWPORT = { width: 1000, height: 800 };

describe("clampMenuToViewport", () => {
  it("leaves a menu that already fits where it was anchored", () => {
    const position = clampMenuToViewport(
      { width: 180, height: 260 },
      { left: 120, top: 200 },
      VIEWPORT,
    );
    assert.deepEqual(position, { left: 120, top: 200 });
  });

  it("lifts a menu anchored near the bottom so its last item stays visible", () => {
    const position = clampMenuToViewport(
      { width: 180, height: 280 },
      { left: 120, top: 700 },
      VIEWPORT,
    );
    assert.deepEqual(position, { left: 120, top: 516 });
    assert.isAtMost(position.top + 280, VIEWPORT.height);
  });

  it("pulls a menu anchored near the right edge back inside", () => {
    const position = clampMenuToViewport(
      { width: 240, height: 120 },
      { left: 960, top: 100 },
      VIEWPORT,
    );
    assert.deepEqual(position, { left: 756, top: 100 });
  });

  it("keeps the margin as a lower bound for negative anchors", () => {
    const position = clampMenuToViewport(
      { width: 180, height: 200 },
      { left: -50, top: -30 },
      VIEWPORT,
    );
    assert.deepEqual(position, { left: 4, top: 4 });
  });

  it("pins a menu taller than the viewport to the top margin", () => {
    const position = clampMenuToViewport(
      { width: 180, height: 900 },
      { left: 120, top: 400 },
      VIEWPORT,
    );
    assert.deepEqual(position, { left: 120, top: 4 });
  });

  it("honors a custom margin", () => {
    const position = clampMenuToViewport(
      { width: 180, height: 280 },
      { left: 990, top: 790 },
      VIEWPORT,
      12,
    );
    assert.deepEqual(position, { left: 808, top: 508 });
  });
});
