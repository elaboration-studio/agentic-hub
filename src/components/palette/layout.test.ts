import { describe, expect, it } from "vitest";

import { PALETTE_FRAME, paletteContentHeight, paletteWindowHeight } from "./layout";

describe("paletteWindowHeight", () => {
  it("adds a uniform frame above and below the panel", () => {
    expect(paletteWindowHeight(300)).toBe(300 + PALETTE_FRAME * 2);
  });

  it("rounds sub-pixel panel heights up so the panel is never clipped", () => {
    expect(paletteWindowHeight(311.4)).toBe(312 + PALETTE_FRAME * 2);
  });
});

describe("paletteContentHeight", () => {
  it("uses the panel height when no confirm dialog is open", () => {
    expect(paletteContentHeight(280, null)).toBe(280);
  });

  it("expands to fit a centered confirm overlay", () => {
    expect(paletteContentHeight(280, 360)).toBe(360 + 48);
  });
});
