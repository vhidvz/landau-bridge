<script setup lang="ts">
const colorMode = useColorMode();
const modes = [
  { value: 'system', icon: 'i-lucide-monitor', label: 'مطابق سیستم' },
  { value: 'light', icon: 'i-lucide-sun', label: 'روشن' },
  { value: 'dark', icon: 'i-lucide-moon', label: 'تیره' },
] as const;
</script>

<template>
  <div class="theme-toggle" role="radiogroup" aria-label="حالت رنگی">
    <button
      v-for="m in modes" :key="m.value" type="button" role="radio" :aria-checked="colorMode.preference === m.value"
      :title="m.label" class="theme-toggle-btn" :class="colorMode.preference === m.value && 'is-on'"
      @click="colorMode.preference = m.value"
    >
      <UIcon :name="m.icon" class="size-4" />
      <span class="sr-only">{{ m.label }}</span>
    </button>
  </div>
</template>
