<script setup lang="ts">
import type { BridgeError } from '~/types/bridge';
const props = defineProps<{ error: BridgeError | null }>();
const title = computed(() => (props.error ? kindLabel(props.error.kind) : ''));
// An unresolved kind falls back to the raw (English) identifier — render that LTR.
const isUnknownKind = computed(() => !!props.error && title.value === props.error.kind);
</script>

<template>
  <div v-if="error" role="alert" class="surface mb-6 border-s-[3px] border-s-sorkh-500 px-4 py-3">
    <p class="font-semibold text-error">
      <span :dir="isUnknownKind ? 'ltr' : undefined">{{ title }}</span>
    </p>
    <p class="mt-0.5 text-sm">{{ error.message }}</p>
    <p v-if="error.detail && error.detail !== error.message" dir="ltr" class="mt-1 text-xs text-muted">{{ error.detail }}</p>
  </div>
</template>
