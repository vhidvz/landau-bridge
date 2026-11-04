/** Long lists are shown a page at a time (LND-Q07): every table, and the review's cards. */

/** Rows per page, sized so a page fits twice the window's height even at the app's 960 × 640
 *  minimum, where rows wrap (measured there: a report row ~108 px, an error ~72, a match ~68, a
 *  review card up to ~370 with its three suggestions). The CSS cap on `.ledger` holds whatever
 *  still grows — an opened error. */
export const PAGE_SIZE = { table: 15, report: 10, cards: 3 } as const;

export const pageCount = (total: number, size: number): number => Math.max(1, Math.ceil(total / Math.max(1, size)));

/** `page` (1-based) kept inside the pages `total` rows make — a list that shrank under it (a part
 *  decided, the last of its page) falls back to its new last page instead of showing nothing. */
export const clampPage = (page: number, total: number, size: number): number =>
  Math.min(Math.max(1, Math.floor(page) || 1), pageCount(total, size));

export const pageSlice = <T>(list: readonly T[], page: number, size: number): T[] => {
  const p = clampPage(page, list.length, size);
  return list.slice((p - 1) * size, p * size);
};

/** The page that shows the row at 0-based `index`. */
export const pageOfIndex = (index: number, size: number): number => Math.floor(Math.max(0, index) / Math.max(1, size)) + 1;

/** The 1-based first and last row a page shows, for «۱ تا ۱۵ از ۴۱۲»; zeros for an empty list. */
export const pageRange = (page: number, total: number, size: number): { from: number; to: number } => {
  if (total <= 0) return { from: 0, to: 0 };
  const p = clampPage(page, total, size);
  return { from: (p - 1) * size + 1, to: Math.min(total, p * size) };
};
