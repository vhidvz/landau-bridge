<script setup lang="ts">
withDefaults(defineProps<{ size?: number; withWordmark?: boolean; dots?: 'accent' | 'current' }>(), {
  size: 28, withWordmark: false, dots: 'accent',
});
</script>

<template>
  <span class="inline-flex items-center gap-3">
    <svg :width="size" :height="size" viewBox="0 0 64 64" fill="none" stroke="currentColor" stroke-width="5" stroke-linecap="round" stroke-linejoin="round" role="img" aria-label="پل لندو">
      <path d="M8 16H56" />
      <path d="M14 16C14 44 50 44 50 16" />
      <g :fill="dots === 'accent' ? 'var(--color-zaferan-400)' : 'currentColor'" stroke="none">
        <circle cx="24" cy="48" r="3.5" />
        <circle cx="32" cy="52" r="3.5" />
        <circle cx="40" cy="48" r="3.5" />
      </g>
    </svg>
    <span v-if="withWordmark" class="text-xl font-extrabold leading-none tracking-tight">پل لندو</span>
  </span>
</template>
