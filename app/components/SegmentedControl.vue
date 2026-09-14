<script setup lang="ts" generic="T extends string">
defineProps<{ items: readonly { value: T; label: string }[]; ariaLabel?: string }>();
const model = defineModel<T>({ required: true });
</script>

<template>
  <div class="segmented" role="radiogroup" :aria-label="ariaLabel">
    <button
      v-for="i in items" :key="i.value" type="button" role="radio" :aria-checked="i.value === model"
      class="segmented-btn" @click="model = i.value"
    >
      {{ i.label }}
    </button>
  </div>
</template>
