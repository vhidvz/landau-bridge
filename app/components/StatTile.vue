<script setup lang="ts">
const props = defineProps<{ label: string; value: number | string; hint?: string; tone?: 'ok' | 'warn' | 'danger' | 'info' }>();
const text = computed(() => (typeof props.value === 'number' ? faNumber(props.value) : props.value));
const toneCls = computed(() => (props.tone ? { ok: 'text-ok', warn: 'text-warn', danger: 'text-danger', info: 'text-info' }[props.tone] : 'text-paper'));
</script>

<template>
  <div class="glass-inset flex min-w-0 flex-col gap-1 px-4 py-3.5">
    <span class="data-label truncate">{{ label }}</span>
    <span class="display tabular text-2xl leading-none" :class="toneCls">{{ text }}</span>
    <span v-if="hint" class="truncate text-xs text-paper-mute">{{ hint }}</span>
  </div>
</template>
