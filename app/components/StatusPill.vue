<script setup lang="ts">
import type { Tone } from '~/utils/fa';

const props = defineProps<{ tone: Tone; label: string }>();
const cls = computed(() => ({
  ok: 'text-ok border-ok/30 bg-ok/6',
  warn: 'text-warn border-warn/30 bg-warn/6',
  danger: 'text-danger border-danger/30 bg-danger/6',
  info: 'text-info border-info/30 bg-info/6',
  muted: 'text-paper-mute border-line-bright bg-white/2',
  brand: 'text-crimson-300 border-crimson-500/35 bg-crimson-500/8',
})[props.tone]);
</script>

<template>
  <span class="inline-flex items-center gap-1.5 whitespace-nowrap rounded-full border px-2.5 py-[4px] text-[0.6875rem] leading-[12px] font-medium" :class="cls">
    <span class="size-1.5 rounded-full bg-current" aria-hidden="true" />
    {{ label }}
  </span>
</template>
