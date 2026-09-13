<script setup lang="ts">
import type { BridgeError } from '~/types/bridge';
const props = defineProps<{ error: BridgeError | null }>();
const title = computed(() => (props.error ? kindLabel(props.error.kind) : ''));
// An unresolved kind falls back to the raw (English) identifier — render that LTR.
const isUnknownKind = computed(() => !!props.error && title.value === props.error.kind);
</script>

<template>
  <UAlert v-if="error" color="error" variant="soft" icon="i-lucide-alert-triangle" class="mb-4">
    <template #title>
      <span v-if="isUnknownKind" dir="ltr">{{ title }}</span>
      <span v-else>{{ title }}</span>
    </template>
    <template #description>
      <p>{{ error.message }}</p>
      <p v-if="error.detail && error.detail !== error.message" dir="ltr" class="mt-1 text-xs opacity-70">{{ error.detail }}</p>
    </template>
  </UAlert>
</template>
