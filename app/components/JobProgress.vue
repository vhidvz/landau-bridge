<script setup lang="ts">
import type { JobEvent, Phase, Progress } from '~/types/bridge';

const props = defineProps<{
  kind: 'IMPORT' | 'REFRESH'; phase: Phase | null; progress: Progress | null; events: JobEvent[]; status: string | null;
}>();
const phases = computed(() => phasesOf(props.kind));
const index = computed(() => (props.phase ? phases.value.indexOf(props.phase) : -1));
const finished = computed(() => props.status === 'DONE');
const percent = computed(() =>
  props.progress && props.progress.total > 0 ? Math.round((props.progress.done / props.progress.total) * 100) : null);
const pierState = (i: number) => (finished.value || i < index.value ? 'done' : i === index.value ? 'current' : 'todo');
const statusColor = computed(() =>
  props.status === 'DONE' ? 'success' : props.status === 'FAILED' ? 'error' : props.status === 'REVIEW' ? 'warning' : 'neutral');
</script>

<template>
  <div class="space-y-5">
    <ol class="stepper" :style="{ '--n': phases.length }" aria-label="مراحل عملیات">
      <li v-for="(p, i) in phases" :key="p" class="stepper-step" :data-state="pierState(i)" :aria-current="i === index && !finished ? 'step' : undefined">
        <span class="stepper-pier" aria-hidden="true" />
        <span class="stepper-label">{{ phaseLabel[p] }}</span>
      </li>
    </ol>

    <div v-if="progress && progress.total > 0" class="space-y-1">
      <UProgress :model-value="percent ?? 0" size="sm" />
      <p class="tabular text-sm text-muted">{{ progress.message }} · {{ faNumber(progress.done) }} از {{ faNumber(progress.total) }}</p>
    </div>

    <div v-if="status" class="flex items-center gap-2 text-sm">
      <span class="text-muted">وضعیت</span>
      <UBadge :color="statusColor" variant="soft">{{ statusLabel[status] ?? status }}</UBadge>
    </div>

    <ul v-if="events.length" class="max-h-64 space-y-1.5 overflow-auto rounded-control bg-elevated/60 p-3 text-sm">
      <li v-for="(e, i) in events" :key="i" class="flex items-start gap-2">
        <span class="level-dot" :data-level="e.level" aria-hidden="true" />
        <span class="min-w-0">
          <span class="text-xs text-muted">
            <template v-if="phaseLabel[e.phase]">{{ phaseLabel[e.phase] }}</template>
            <bdi v-else dir="ltr">{{ e.phase }}</bdi>
          </span>
          <span class="block" :class="e.level === 'ERROR' ? 'text-error' : e.level === 'WARN' ? 'text-warning' : ''">{{ e.message }}</span>
        </span>
      </li>
    </ul>
  </div>
</template>
