import type { CandidateView, Inspection, Reason } from '~/types/bridge';

/** What the second confirm shows: the core's reasons, the product picked, and the money. */
export interface PendingConfirm {
  supplierName: string;
  reasons: Reason[];
  /** Named from the list the seller picked in, so the confirm shows it even before any money read. */
  product: { name: string; brand: string | null };
  candidate: CandidateView | null;
  inspection: Inspection | null;
}

/**
 * A seller's pick with R-d's second confirm. The pick is always sent first without a confirmation
 * and the core decides: a `ConfirmRequired` refusal opens the confirm with exactly the reasons the
 * core named, and only the seller's "yes" sends it again with them. So the screen never decides on
 * its own that a pick is consistent.
 */
export function usePick() {
  const pending = ref<PendingConfirm | null>(null);
  /** A pick in flight, its confirm included: a second one waits for it to end. */
  const busy = ref(false);
  let answer: ((ok: boolean) => void) | null = null;

  /** `true` when the pick landed, `false` when the seller backed out or another pick is in flight;
   *  other errors are thrown. */
  const pick = async (
    send: (confirmed?: Reason[]) => Promise<void>,
    context: Omit<PendingConfirm, 'reasons'>,
  ): Promise<boolean> => {
    if (busy.value) return false;
    busy.value = true;
    try {
      try {
        await send();
        return true;
      } catch (e) {
        const reasons = confirmReasons(e);
        if (!reasons) throw e;
        const ok = await new Promise<boolean>((resolve) => {
          answer = resolve;
          pending.value = { ...context, reasons };
        });
        pending.value = null;
        answer = null;
        if (!ok) return false;
        await send(reasons);
        return true;
      }
    } finally {
      busy.value = false;
    }
  };

  const respond = (ok: boolean) => answer?.(ok);
  return { pending, busy, pick, respond };
}
