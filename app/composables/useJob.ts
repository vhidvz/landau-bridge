import type { JobDetail, JobEvent, Phase, Progress } from '~/types/bridge';

export function useJob() {
  const jobId = ref<number | null>(null);
  const detail = ref<JobDetail | null>(null);
  const phase = ref<Phase | null>(null);
  const progress = ref<Progress | null>(null);
  const events = ref<JobEvent[]>([]);
  const status = ref<string | null>(null);
  let off: (() => void) | null = null;

  const reload = async () => {
    if (jobId.value == null) return;
    detail.value = await bridge.jobGet(jobId.value);
    status.value = detail.value.job.status;
    phase.value = detail.value.job.phase;
  };

  const follow = async (id: number) => {
    jobId.value = id;
    events.value = [];
    progress.value = null;
    await reload();
    off?.();
    off = await onJobEvents({
      progress: (p) => { if (p.job_id === id) { progress.value = p; phase.value = p.phase; } },
      event: (e) => { if (e.job_id === id) events.value.unshift(e); },
      done: async (d) => { if (d.job_id === id) { status.value = d.status; await reload(); } },
    });
  };

  onBeforeUnmount(() => off?.());
  return { jobId, detail, phase, progress, events, status, follow, reload };
}
