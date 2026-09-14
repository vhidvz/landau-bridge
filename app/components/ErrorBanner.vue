<script setup lang="ts">
import type { BridgeError } from '~/types/bridge';
const props = defineProps<{ error: BridgeError | null }>();
const title = computed(() => (props.error ? kindLabel(props.error.kind) : ''));
// An unresolved kind falls back to the raw (English) identifier — render that LTR.
const isUnknownKind = computed(() => !!props.error && title.value === props.error.kind);
</script>

<template>
  <div v-if="error" role="alert" class="glass mb-6 px-4 py-3" style="border-inline-start: 3px solid var(--color-danger)">
    <p class="font-semibold text-danger"><span :dir="isUnknownKind ? 'ltr' : undefined">{{ title }}</span></p>
    <p class="mt-0.5 text-sm text-paper-dim">{{ error.message }}</p>
    <template v-if="error.detail && error.detail !== error.message">
      <p class="data-label mt-2">جزئیات فنی</p>
      <p dir="ltr" class="font-mono text-xs break-all text-paper-mute">{{ error.detail }}</p>
    </template>
  </div>
</template>
