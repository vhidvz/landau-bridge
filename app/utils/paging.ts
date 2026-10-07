/** Long lists are shown a page at a time (LND-Q07): the matches, the undecided parts, a review. */

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
