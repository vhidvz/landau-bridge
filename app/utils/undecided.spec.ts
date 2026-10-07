import { describe, expect, it } from 'vitest';
import { nextAfterDecision, nextAfterSkip } from './undecided';

const list = ['a', 'b', 'c'].map((name_key) => ({ name_key }));

describe('walking the undecided list', () => {
  it('opens the part that followed the decided one, then wraps, then stops', () => {
    expect(nextAfterDecision(list, 'a')?.name_key).toBe('b');
    expect(nextAfterDecision(list, 'b')?.name_key).toBe('c');
    expect(nextAfterDecision(list, 'c')?.name_key).toBe('a');
    expect(nextAfterDecision([{ name_key: 'a' }], 'a')).toBeNull();
    expect(nextAfterDecision(list, 'gone')?.name_key).toBe('a');
  });

  it('moves past a part put off for later and wraps to the top', () => {
    expect(nextAfterSkip(list, 'a')?.name_key).toBe('b');
    expect(nextAfterSkip(list, 'c')?.name_key).toBe('a');
    expect(nextAfterSkip([{ name_key: 'a' }], 'a')).toBeNull();
  });
});
