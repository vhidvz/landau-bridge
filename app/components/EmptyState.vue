<script setup lang="ts">
withDefaults(defineProps<{ title: string; description?: string; art?: 'bridge' | 'shelf' }>(), { art: 'bridge' });
</script>

<template>
  <div class="flex flex-col items-center gap-3 px-6 py-12 text-center">
    <svg v-if="art === 'bridge'" width="96" height="48" viewBox="0 0 96 48" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" class="text-petrol-300 dark:text-petrol-700" aria-hidden="true">
      <path d="M4 22H92" />
      <path d="M20 22v10M76 22v10" />
      <path d="M20 32Q48 58 76 32" />
      <path d="M6 42q6-3 12 0t12 0 12 0 12 0 12 0 12 0 12 0" class="text-firoozeh-300 dark:text-firoozeh-800" />
    </svg>
    <svg v-else width="96" height="48" viewBox="0 0 96 48" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" class="text-petrol-300 dark:text-petrol-700" aria-hidden="true">
      <path d="M8 12H88M8 28H88M8 44H88" />
      <path d="M8 6v40M88 6v40" />
      <rect x="18" y="16" width="14" height="12" rx="1" class="text-firoozeh-300 dark:text-firoozeh-800" />
    </svg>
    <p class="font-semibold text-highlighted">{{ title }}</p>
    <p v-if="description" class="max-w-prose text-sm text-muted">{{ description }}</p>
    <div v-if="$slots.action" class="mt-1"><slot name="action" /></div>
  </div>
</template>
