<script setup lang="ts">
import type { JobEvent, Phase, Progress } from '~/types/bridge';
const props = defineProps<{ phase: Phase | null; progress: Progress | null; events: JobEvent[]; status: string | null }>();
const phases: Phase[] = ['PREFLIGHT', 'FETCH', 'AGGREGATE', 'MATCH', 'REVIEW', 'PRICE', 'WRITE', 'REPRICE', 'REPORT'];
const index = computed(() => (props.phase ? phases.indexOf(props.phase) : -1));
const percent = computed(() => (props.progress && props.progress.total > 0 ? Math.round((props.progress.done / props.progress.total) * 100) : null));
</script>

<template>
  <div class="space-y-3">
    <ol class="flex flex-wrap gap-2 text-sm">
      <li v-for="(p, i) in phases" :key="p" class="rounded-full px-3 py-1" :class="i < index ? 'bg-emerald-100 text-emerald-900' : i === index ? 'bg-primary-100 text-primary-900 font-semibold' : 'bg-neutral-100 text-neutral-500'">
        {{ phaseLabel[p] }}
      </li>
    </ol>
    <div v-if="progress && progress.total > 0">
      <UProgress :model-value="percent ?? 0" />
      <p class="mt-1 text-sm text-neutral-600">{{ progress.message }} — {{ faNumber(progress.done) }} از {{ faNumber(progress.total) }}</p>
    </div>
    <p v-if="status" class="text-sm">وضعیت: <UBadge :color="status === 'DONE' ? 'success' : status === 'FAILED' ? 'error' : 'neutral'">{{ statusLabel[status] ?? status }}</UBadge></p>
    <ul class="max-h-64 space-y-1 overflow-auto rounded-lg border border-neutral-200 p-2 text-sm dark:border-neutral-800">
      <li v-for="(e, i) in events" :key="i" :class="e.level === 'ERROR' ? 'text-red-700' : e.level === 'WARN' ? 'text-amber-700' : 'text-neutral-700'">
        <span class="font-mono text-xs">[<template v-if="phaseLabel[e.phase]">{{ phaseLabel[e.phase] }}</template><bdi v-else dir="ltr">{{ e.phase }}</bdi>]</span> {{ e.message }}
      </li>
    </ul>
  </div>
</template>
