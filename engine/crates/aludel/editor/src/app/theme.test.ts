import { describe, expect, it } from "vitest";
import { DEFAULT_PAGE_THEME, normalizePageTheme, pageThemesEqual } from "./theme";

describe("page theme", () => {
  it("normalizes missing and unsafe values to stable defaults", () => {
    expect(normalizePageTheme({ orientation: "other", margins: { top: 100 } })).toMatchObject({
      ...DEFAULT_PAGE_THEME, margins: { ...DEFAULT_PAGE_THEME.margins, top: 60 },
    });
  });

  it("compares normalized snapshots structurally", () => {
    expect(pageThemesEqual(DEFAULT_PAGE_THEME, normalizePageTheme(DEFAULT_PAGE_THEME))).toBe(true);
  });
});
