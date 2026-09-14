<script setup lang="ts">
const props = defineProps<{ value: number }>();
const clamped = computed(() => Math.max(0, Math.min(100, props.value)));
</script>

<template>
  <span class="tabular inline-flex items-center gap-2" :title="`شباهت ${faNumber(clamped)} درصد`">
    <span class="sim-track" aria-hidden="true">
      <span class="sim-fill" :data-tone="similarityTone(clamped)" :style="{ width: `${clamped}%` }" />
    </span>
    <span class="text-xs text-muted">{{ faNumber(clamped) }}٪</span>
  </span>
</template>
