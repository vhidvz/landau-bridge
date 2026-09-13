import type { JobDetail, JobEvent, Phase, Progress } from '~/types/bridge';

export function useJob() {
  const jobId = ref<number | null>(null);
  const detail = ref<JobDetail | null>(null);
  const phase = ref<Phase | null>(null);
  const progress = ref<Progress | null>(null);
  const events = ref<JobEvent[]>([]);
  const status = ref<string | null>(null);
  let off: (() => void) | null = null;
  const seq = createSequencer();

  // `id`/`mine` are captured at call time so a reload started for an older
  // `follow()` can never clobber state once a newer one has taken over. A bare
  // `reload()` call (no args) just snapshots today's job/generation instead of
  // starting a new one, so it never invalidates an in-flight `follow()`.
  const reload = async (id: number | null = jobId.value, mine: number = seq.current()) => {
    if (id == null) return;
    const d = await bridge.jobGet(id);
    if (!seq.isCurrent(mine)) return;
    detail.value = d;
    status.value = d.job.status;
    phase.value = d.job.phase;
  };

  const follow = async (id: number) => {
    const mine = seq.start();
    const prev = off;
    off = null;
    prev?.();

    jobId.value = id;
    events.value = [];
    progress.value = null;

    await reload(id, mine);
    if (!seq.isCurrent(mine)) return;

    const unlisten = await onJobEvents({
      progress: (p) => { if (seq.isCurrent(mine) && p.job_id === id) { progress.value = p; phase.value = p.phase; } },
      event: (e) => { if (seq.isCurrent(mine) && e.job_id === id) events.value.unshift(e); },
      done: async (d) => {
        if (d.job_id !== id) return;
        if (seq.isCurrent(mine)) status.value = d.status;
        await reload(id, mine);
      },
    });

    if (!seq.isCurrent(mine)) {
      unlisten();
      return;
    }
    off = unlisten;
  };

  onBeforeUnmount(() => off?.());
  return { jobId, detail, phase, progress, events, status, follow, reload };
}
