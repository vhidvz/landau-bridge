import { describe, expect, it } from 'vitest';
import { similarityTone } from './similarity';

describe('similarityTone', () => {
  it('maps the spec thresholds', () => {
    expect(similarityTone(100)).toBe('good');
    expect(similarityTone(80)).toBe('good');
    expect(similarityTone(79)).toBe('fair');
    expect(similarityTone(60)).toBe('fair');
    expect(similarityTone(59)).toBe('weak');
    expect(similarityTone(0)).toBe('weak');
  });
});
