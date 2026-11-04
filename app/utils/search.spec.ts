import { describe, expect, it } from 'vitest';
import { matchesQuery, searchKey } from './search';

describe('search across Persian text', () => {
  it('treats Arabic ي/ك, Persian/Arabic/Latin digits, ZWNJ and case alike', () => {
    expect(searchKey('لنت ترمز ۲۰۶')).toBe(searchKey('لنت ترمز 206'));
    expect(searchKey('كمك فنر')).toBe(searchKey('کمک فنر'));
    expect(searchKey('پيكان')).toBe(searchKey('پیکان'));
    expect(searchKey('به‌روزرسانی')).toBe(searchKey('بهروزرسانی'));
    expect(searchKey('  NGK   bkr6e ')).toBe('ngk bkr6e');
    expect(searchKey('٢٠٦')).toBe('206');
  });

  it('matches when any field contains the query, and an empty query matches everything', () => {
    expect(matchesQuery('ترمز ۲۰۶', ['لنت ترمز 206 جلو', null])).toBe(true);
    expect(matchesQuery('bp-104', [undefined, 'BP-10422'])).toBe(true);
    expect(matchesQuery('پراید', ['تیبا'])).toBe(false);
    expect(matchesQuery('', [])).toBe(true);
    expect(matchesQuery('   ', ['x'])).toBe(true);
  });
});
