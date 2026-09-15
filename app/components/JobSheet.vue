<script setup lang="ts">
import type { JobEvent, JobRow, Phase, Progress } from '~/types/bridge';

const props = defineProps<{
  job: JobRow;
  storeName?: string | null;
  live?: { phase: Phase | null; progress: Progress | null; events: JobEvent[]; status: string | null } | null;
}>();

const status = computed(() => props.live?.status ?? props.job.status);
const phase = computed(() => props.live?.phase ?? props.job.phase);
const tone = computed(() => statusTone(status.value));
const active = computed(() => status.value === 'RUNNING' || status.value === 'REVIEW');
const strip = computed(() => ({ ok: 'bg-ok/6', danger: 'bg-danger/6', warn: 'bg-warn/6', info: 'bg-info/6', muted: 'bg-white/2', brand: 'bg-crimson-500/6' })[tone.value]);
const circle = computed(() => ({ ok: 'bg-ok/15 text-ok', danger: 'bg-danger/15 text-danger', warn: 'bg-warn/15 text-warn', info: 'bg-info/15 text-info', muted: 'bg-white/8 text-paper-mute', brand: 'bg-crimson-500/15 text-crimson-300' })[tone.value]);
const icon = computed(() => ({ ok: 'i-lucide-check', danger: 'i-lucide-x', warn: 'i-lucide-hourglass', info: 'i-lucide-loader', muted: 'i-lucide-minus', brand: 'i-lucide-check' })[tone.value]);

const facts = computed(() => {
  const j = props.job;
  const out: { label: string; value: string; ltr?: boolean }[] = [];
  if (j.kind === 'IMPORT') out.push({ label: 'حالت', value: j.mode === 'ZERO' ? 'با تعداد صفر، برای بازبینی' : 'با تعداد، از تاریخ' });
  if (j.since_date) out.push({ label: 'از تاریخ', value: j.since_date, ltr: true });
  out.push({ label: 'فروشگاه', value: props.storeName ?? j.store });
  if (j.kind === 'REFRESH') out.push({ label: 'بازنویسی قیمت دستی', value: j.overwrite ? 'بله' : 'خیر' });
  const invoices = j.summary?.invoice_ids;
  if (Array.isArray(invoices)) out.push({ label: 'فاکتورها', value: faNumber(invoices.length) });
  out.push({ label: 'شروع', value: isoToLocal(j.started_at) });
  if (j.finished_at) out.push({ label: 'پایان', value: isoToLocal(j.finished_at) });
  return out;
});
const tiles = computed(() => summaryTiles(props.job));
</script>

<template>
  <GlassCard :pad="false" class="overflow-hidden">
    <div class="flex items-center justify-between gap-4 border-b hairline px-6 py-4" :class="strip">
      <div class="flex items-center gap-3">
        <span class="flex size-8 items-center justify-center rounded-full" :class="circle" aria-hidden="true">
          <UIcon :name="icon" class="size-4" :class="tone === 'info' && 'animate-spin'" />
        </span>
        <div class="leading-tight">
          <div class="text-sm font-medium text-paper">{{ statusLabel[status] ?? status }}</div>
          <div class="text-xs text-paper-mute">مرحله: {{ phaseLabel[phase] ?? phase }}</div>
        </div>
      </div>
      <div class="text-end text-xs text-paper-mute">
        <div>عملیات <span class="tabular">{{ faNumber(job.id) }}</span></div>
        <div class="tabular">{{ faDuration(job.started_at, job.finished_at) }}</div>
      </div>
    </div>
    <div class="space-y-5 px-6 py-5">
      <dl class="grid grid-cols-2 gap-x-8 gap-y-3 sm:grid-cols-3">
        <div v-for="f in facts" :key="f.label">
          <dt class="data-label mb-0.5">{{ f.label }}</dt>
          <dd class="tabular text-[0.8125rem] text-paper-dim"><bdi v-if="f.ltr" dir="ltr">{{ f.value }}</bdi><template v-else>{{ f.value }}</template></dd>
        </div>
      </dl>
      <JobProgress v-if="active && live" :kind="job.kind" :phase="live.phase" :progress="live.progress" :events="live.events" :status="live.status" />
      <div v-else-if="tiles.length" class="grid grid-cols-3 gap-2 md:grid-cols-6">
        <StatTile v-for="t in tiles" :key="t.label" :label="t.label" :value="t.value" :tone="t.label === 'ناموفق' && t.value > 0 ? 'danger' : undefined" />
      </div>
    </div>
  </GlassCard>
</template>
