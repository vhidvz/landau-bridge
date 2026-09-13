<script setup lang="ts">
import type { BridgeError } from '~/types/bridge';
defineProps<{ error: BridgeError | null }>();
</script>

<template>
  <UAlert v-if="error" color="error" variant="soft" icon="i-lucide-alert-triangle" :title="kindLabel(error.kind)" class="mb-4">
    <template #description>
      <p>{{ error.message }}</p>
      <p v-if="error.detail && error.detail !== error.message" dir="ltr" class="mt-1 text-xs opacity-70">{{ error.detail }}</p>
    </template>
  </UAlert>
</template>
