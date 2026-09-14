<script setup lang="ts">
import type { JobRow } from '~/types/bridge';

const props = defineProps<{ job: JobRow; compact?: boolean }>();
const kindText = computed(() => jobKindLabel(props.job));
const stateText = computed(() => (props.job.status === 'REVIEW' ? statusLabel.REVIEW : (phaseLabel[props.job.phase] ?? props.job.phase)));
const to = computed(() => (props.job.kind === 'IMPORT' ? `/import?job=${props.job.id}` : '/refresh'));
</script>

<template>
  <NuxtLink :to="to" class="glass-inset hover-glow flex items-center gap-2.5 px-3.5 py-3">
    <span class="dot bg-crimson-400 text-crimson-400" aria-hidden="true" />
    <span class="min-w-0 flex-1 leading-tight">
      <span class="block truncate font-medium text-paper-dim" :class="compact ? 'text-xs' : 'text-sm'">{{ kindText }} <span class="tabular">{{ faNumber(job.id) }}</span></span>
      <span class="block truncate text-paper-mute" :class="compact ? 'text-[0.6875rem]' : 'text-xs'">{{ stateText }}</span>
    </span>
    <UIcon name="i-lucide-arrow-left" class="size-3.5 shrink-0 text-paper-mute" />
  </NuxtLink>
</template>
