/**
 * Coordinates overlapping async attempts so only the most recently started one's
 * effects apply — the mechanism behind `useJob().follow()`'s re-entrancy guard.
 * Pure and framework-free so it can be unit-tested without a Vue/Nuxt runtime.
 */
export function createSequencer() {
  let generation = 0;
  return {
    /** Begin a new attempt; invalidates every earlier one. Returns this attempt's token. */
    start: (): number => ++generation,
    /** The token of whichever attempt is current right now, without starting a new one. */
    current: (): number => generation,
    /** True when `token` is still the most recently started (or current) attempt. */
    isCurrent: (token: number): boolean => token === generation,
  };
}
