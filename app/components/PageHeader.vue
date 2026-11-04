<script setup lang="ts">
// A page's name and, at most, one plain sentence about it — no tagline, no section number. The
// actions sit level with the bottom of that text block, with or without the sentence.
defineProps<{ kicker?: string; title: string; lede?: string }>();
</script>

<template>
  <header class="mb-8 flex flex-wrap items-end justify-between gap-x-6 gap-y-4">
    <div class="min-w-0">
      <p v-if="kicker" class="kicker mb-3" data-reveal>{{ kicker }}</p>
      <h1 class="display text-[2.4rem] leading-[1.2] text-paper" data-reveal>{{ title }}</h1>
      <p v-if="lede" class="mt-3 max-w-[58ch] text-[0.95rem] leading-relaxed text-paper-dim" data-reveal>{{ lede }}</p>
    </div>
    <div v-if="$slots.actions" class="flex shrink-0 items-center gap-2" data-reveal><slot name="actions" /></div>
  </header>
</template>
