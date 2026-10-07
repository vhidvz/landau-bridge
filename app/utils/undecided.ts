/** Walking the «بدون تصمیم» list one part after another (LND-Q07). */
interface Keyed { name_key: string }

/** The part to open after `key` was decided and leaves the list: the one that followed it, else
 *  the first left — `null` once nothing else remains. */
export const nextAfterDecision = <T extends Keyed>(shown: readonly T[], key: string): T | null => {
  const at = shown.findIndex((u) => u.name_key === key);
  const rest = shown.filter((u) => u.name_key !== key);
  return rest[Math.max(at, 0)] ?? rest[0] ?? null;
};

/** The part to open after `key` was put off with «بعداً»: it stays listed, so the walk moves on
 *  and wraps to the top — `null` when it is the only one. */
export const nextAfterSkip = <T extends Keyed>(shown: readonly T[], key: string): T | null => {
  if (shown.length < 2) return null;
  const at = shown.findIndex((u) => u.name_key === key);
  return shown[(at + 1) % shown.length] ?? null;
};
