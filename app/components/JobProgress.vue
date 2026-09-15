<script setup lang="ts">
import type { JobEvent, Phase, Progress } from '~/types/bridge';

const props = withDefaults(defineProps<{
  kind: 'IMPORT' | 'REFRESH'; phase: Phase | null; progress: Progress | null; events: JobEvent[]; status: string | null;
  showStatus?: boolean;
}>(), { showStatus: true });
const phases = computed(() => phasesOf(props.kind));
const index = computed(() => (props.phase ? phases.value.indexOf(props.phase) : -1));
// status also drives the stepper's pier states below — pass the real status even when
// showStatus is false, never null just to hide the row, or a finished/failed run looks stuck mid-step.
const finished = computed(() => props.status === 'DONE');
const halted = computed(() => ['FAILED', 'CANCELLED', 'INTERRUPTED'].includes(props.status ?? ''));
const percent = computed(() =>
  props.progress && props.progress.total > 0 ? Math.round((props.progress.done / props.progress.total) * 100) : null);
const pierState = (i: number) =>
  finished.value || i < index.value ? 'done' : i === index.value && halted.value ? 'failed' : i === index.value ? 'current' : 'todo';
</script>

<template>
  <div class="space-y-5">
    <ol class="stepper" :style="{ '--n': phases.length }" aria-label="مراحل عملیات">
      <li v-for="(p, i) in phases" :key="p" class="stepper-step" :data-state="pierState(i)" :aria-current="i === index && !finished && !halted ? 'step' : undefined">
        <span class="stepper-pier" aria-hidden="true" />
        <span class="stepper-label">{{ phaseLabel[p] }}</span>
      </li>
    </ol>

    <div v-if="progress && progress.total > 0" class="space-y-1.5">
      <UProgress :model-value="percent ?? 0" size="sm" />
      <p class="tabular text-sm text-paper-mute">{{ progress.message }} · {{ faNumber(progress.done) }} از {{ faNumber(progress.total) }}</p>
    </div>

    <div v-if="status && showStatus" class="flex items-center gap-2 text-sm">
      <span class="text-paper-mute">وضعیت</span>
      <StatusPill :tone="statusTone(status)" :label="statusLabel[status] ?? status" />
    </div>

    <div v-if="events.length" class="glass-inset max-h-64 overflow-auto">
      <EventTimeline :events="events" :kind="kind" />
    </div>
  </div>
</template>
