import { describe, expect, it } from 'vitest';
import { clampPage, pageCount, pageOfIndex, pageSlice } from './paging';

const list = Array.from({ length: 120 }, (_, i) => i);

describe('paging long lists', () => {
  it('cuts pages of the given size, the last one short', () => {
    expect(pageCount(120, 50)).toBe(3);
    expect(pageSlice(list, 1, 50)).toEqual(list.slice(0, 50));
    expect(pageSlice(list, 3, 50)).toEqual(list.slice(100, 120));
  });

  it('keeps a page inside the list as it shrinks or empties', () => {
    expect(clampPage(3, 100, 50)).toBe(2);
    expect(clampPage(5, 0, 50)).toBe(1);
    expect(clampPage(0, 120, 50)).toBe(1);
    expect(pageSlice(list.slice(0, 100), 3, 50)).toEqual(list.slice(50, 100));
    expect(pageSlice([], 2, 50)).toEqual([]);
    expect(pageCount(0, 50)).toBe(1);
  });

  it('finds the page of a row', () => {
    expect(pageOfIndex(0, 50)).toBe(1);
    expect(pageOfIndex(49, 50)).toBe(1);
    expect(pageOfIndex(50, 50)).toBe(2);
  });
});
