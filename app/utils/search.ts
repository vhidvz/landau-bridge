import { latinDigits } from './fa';

/** One form for a search box and the text it searches: a seller types «كمك ۲۰۶» on one keyboard
 *  and the invoice says «کمک 206» — Arabic ي/ك and Persian ی/ک are different code points that
 *  look the same, digits arrive in three scripts, and a ZWNJ may or may not sit inside a word. */
export const searchKey = (s: string): string =>
  latinDigits(s)
    .replace(/[يى]/g, 'ی')
    .replace(/ك/g, 'ک')
    .replace(/[‌‍ـ]/g, '')
    .replace(/\s+/g, ' ')
    .trim()
    .toLowerCase();

/** True when any field holds the query; a blank query matches every row. */
export const matchesQuery = (query: string, fields: readonly (string | null | undefined)[]): boolean => {
  const q = searchKey(query);
  return !q || fields.some((f) => !!f && searchKey(f).includes(q));
};
