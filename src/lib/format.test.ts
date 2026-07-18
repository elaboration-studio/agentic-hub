import { describe, expect, it } from "vitest";

import { formatLocalTimestamp } from "./format";

describe("formatLocalTimestamp", () => {
  it("renders UTC timestamps in the browser's local date and time format", () => {
    const timestamp = "2026-07-16T17:47:59Z";

    expect(formatLocalTimestamp(timestamp)).toBe(
      new Date(timestamp).toLocaleString(undefined, {
        dateStyle: "medium",
        timeStyle: "medium",
      }),
    );
    expect(formatLocalTimestamp(timestamp)).not.toBe(timestamp);
  });

  it("returns an invalid timestamp unchanged", () => {
    const timestamp = "not-a-timestamp";

    expect(formatLocalTimestamp(timestamp)).toBe(timestamp);
  });
});
