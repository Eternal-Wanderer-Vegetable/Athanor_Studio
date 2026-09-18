// This file is part of Athanor, the Azodoc document engine.

export type PageSize = "A4" | "Letter";
export type PageOrientation = "portrait" | "landscape";

export interface PageTheme {
  [key: string]: unknown;
  pageSize: PageSize;
  orientation: PageOrientation;
  margins: { top: number; right: number; bottom: number; left: number };
  header: string;
  footer: string;
}

export const DEFAULT_PAGE_THEME: PageTheme = {
  pageSize: "A4",
  orientation: "portrait",
  margins: { top: 22, right: 18, bottom: 22, left: 18 },
  header: "",
  footer: "",
};

function numberInRange(value: unknown, fallback: number): number {
  const n = typeof value === "number" && Number.isFinite(value) ? value : fallback;
  return Math.max(5, Math.min(60, Math.round(n * 10) / 10));
}

export function normalizePageTheme(value: unknown): PageTheme {
  const source = value && typeof value === "object" ? value as Record<string, unknown> : {};
  const defaults = source.defaults && typeof source.defaults === "object" ? source.defaults as Record<string, unknown> : {};
  const page = defaults.page && typeof defaults.page === "object" ? defaults.page as Record<string, unknown> : {};
  const pageSpec = typeof page.size === "string" ? page.size.split(/\s+/) : [];
  const pageMargin = typeof page.margin === "string" ? page.margin.match(/[\d.]+/g)?.map(Number) ?? [] : [];
  const margins = source.margins && typeof source.margins === "object"
    ? source.margins as Record<string, unknown>
    : {};
  return {
    ...source,
    pageSize: source.pageSize === "Letter" || pageSpec[0] === "Letter" ? "Letter" : "A4",
    orientation: source.orientation === "landscape" || pageSpec[1] === "landscape" ? "landscape" : "portrait",
    margins: {
      top: numberInRange(margins.top, pageMargin[0] ?? DEFAULT_PAGE_THEME.margins.top),
      right: numberInRange(margins.right, pageMargin[3] ?? pageMargin[1] ?? DEFAULT_PAGE_THEME.margins.right),
      bottom: numberInRange(margins.bottom, pageMargin[2] ?? pageMargin[0] ?? DEFAULT_PAGE_THEME.margins.bottom),
      left: numberInRange(margins.left, pageMargin[3] ?? pageMargin[1] ?? DEFAULT_PAGE_THEME.margins.left),
    },
    header: typeof source.header === "string" ? source.header.slice(0, 200) : "",
    footer: typeof source.footer === "string" ? source.footer.slice(0, 200) : "",
    defaults: {
      ...defaults,
      page: {
        ...page,
        size: `${source.pageSize === "Letter" || pageSpec[0] === "Letter" ? "Letter" : "A4"} ${source.orientation === "landscape" || pageSpec[1] === "landscape" ? "landscape" : "portrait"}`,
        margin: `${numberInRange(margins.top, pageMargin[0] ?? DEFAULT_PAGE_THEME.margins.top)}mm ${numberInRange(margins.right, pageMargin[3] ?? pageMargin[1] ?? DEFAULT_PAGE_THEME.margins.right)}mm ${numberInRange(margins.bottom, pageMargin[2] ?? pageMargin[0] ?? DEFAULT_PAGE_THEME.margins.bottom)}mm ${numberInRange(margins.left, pageMargin[3] ?? pageMargin[1] ?? DEFAULT_PAGE_THEME.margins.left)}mm`,
      },
    },
  };
}

export function pageThemesEqual(a: PageTheme, b: PageTheme): boolean {
  return JSON.stringify(normalizePageTheme(a)) === JSON.stringify(normalizePageTheme(b));
}
