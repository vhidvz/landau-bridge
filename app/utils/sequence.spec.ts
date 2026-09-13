import { describe, expect, it } from 'vitest';
import { createSequencer } from './sequence';

describe('createSequencer', () => {
  it('treats the first started attempt as current until a newer one starts', () => {
    const seq = createSequencer();
    const a = seq.start();
    expect(seq.isCurrent(a)).toBe(true);
  });

  it('invalidates an older attempt once a newer one starts', () => {
    const seq = createSequencer();
    const a = seq.start();
    const b = seq.start();
    expect(seq.isCurrent(a)).toBe(false);
    expect(seq.isCurrent(b)).toBe(true);
  });

  it('exposes the current token without starting a new attempt', () => {
    const seq = createSequencer();
    const a = seq.start();
    expect(seq.current()).toBe(a);
    expect(seq.isCurrent(seq.current())).toBe(true);
  });

  it('models useJob().follow() re-entrancy: a slow first attempt loses to a fast second one', async () => {
    const seq = createSequencer();
    const applied: number[] = [];

    const attempt = async (token: number, id: number, delayMs: number) => {
      await new Promise((resolve) => setTimeout(resolve, delayMs));
      if (seq.isCurrent(token)) applied.push(id);
    };

    const first = seq.start();
    const slowFirst = attempt(first, 1, 20);
    const second = seq.start();
    const fastSecond = attempt(second, 2, 0);

    await Promise.all([slowFirst, fastSecond]);
    expect(applied).toEqual([2]);
  });
});
