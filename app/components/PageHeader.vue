<script setup lang="ts">
defineProps<{ kicker: string; title: string; lede?: string }>();
</script>

<template>
  <header class="mb-10">
    <p class="kicker mb-3" data-reveal>{{ kicker }}</p>
    <h1 class="display text-[2.4rem] leading-[1.2] text-paper" data-reveal>{{ title }}</h1>
    <div class="mt-4 flex items-end justify-between gap-6">
      <p v-if="lede" class="max-w-[58ch] text-[0.95rem] leading-relaxed text-paper-dim" data-reveal>{{ lede }}</p>
      <div v-if="$slots.actions" class="ms-auto flex shrink-0 items-center gap-2" data-reveal><slot name="actions" /></div>
    </div>
  </header>
</template>
