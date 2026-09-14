<script setup lang="ts">
import type { JobRow } from '~/types/bridge';

const props = defineProps<{ job: JobRow; compact?: boolean }>();
const kindText = computed(() => (props.job.kind === 'IMPORT' ? 'وارد کردن فاکتورها' : 'به‌روزرسانی قیمت‌ها'));
const stateText = computed(() =>
  props.job.status === 'REVIEW' ? statusLabel.REVIEW : (phaseLabel[props.job.phase] ?? props.job.phase));
const to = computed(() => (props.job.kind === 'IMPORT' ? `/import?job=${props.job.id}` : '/refresh'));
</script>

<template>
  <NuxtLink :to="to" class="running-card" :class="!compact && 'text-base'">
    <span class="running-dot" aria-hidden="true" />
    <span class="min-w-0 flex-1">
      <span class="block truncate font-medium">
        {{ kindText }} <span class="tabular">{{ faNumber(job.id) }}</span>
      </span>
      <span class="block truncate opacity-80" :class="compact ? 'text-xs' : 'text-sm'">{{ stateText }}</span>
    </span>
    <UIcon name="i-lucide-arrow-left" class="size-4 shrink-0 opacity-70" />
  </NuxtLink>
</template>
