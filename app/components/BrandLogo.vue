<script setup lang="ts">
/**
 * The bridge mark: deck, towerless bowl, three dots. `brand` colours it after the Landau ل —
 * sapphire bowl, crimson dots — through CSS-styled gradient stops (a `stop-color` attribute
 * cannot read a CSS variable). Gradient ids are per instance so several marks can share a page.
 */
withDefaults(defineProps<{ size?: number; tone?: 'mono' | 'brand'; withWordmark?: boolean; decorative?: boolean }>(), {
  size: 28, tone: 'brand', withWordmark: false, decorative: false,
});
const gradId = `bowl-${useId()}`;
</script>

<template>
  <span class="inline-flex items-center gap-3">
    <svg
      :width="size" :height="size" viewBox="0 0 64 64" fill="none" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"
      class="brand-mark" :class="`brand-mark--${tone}`"
      :role="withWordmark || decorative ? undefined : 'img'"
      :aria-label="withWordmark || decorative ? undefined : 'پل لاندا'"
      :aria-hidden="withWordmark || decorative ? 'true' : undefined"
    >
      <defs v-if="tone === 'brand'">
        <linearGradient :id="gradId" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" class="brand-mark__bowl-lit" />
          <stop offset="1" class="brand-mark__bowl-deep" />
        </linearGradient>
      </defs>
      <path d="M8 16H56" class="brand-mark__deck" />
      <path d="M14 16C14 44 50 44 50 16" class="brand-mark__bowl" :stroke="tone === 'brand' ? `url(#${gradId})` : undefined" />
      <g class="brand-mark__dots" stroke="none">
        <circle cx="24" cy="48" r="3.5" />
        <circle cx="32" cy="52" r="3.5" />
        <circle cx="40" cy="48" r="3.5" />
      </g>
    </svg>
    <span v-if="withWordmark" class="display text-xl leading-none text-paper">پل لاندا</span>
  </span>
</template>

<style scoped>
.brand-mark { display: block; flex: none; }
.brand-mark--mono .brand-mark__deck, .brand-mark--mono .brand-mark__bowl { stroke: currentColor; }
.brand-mark--mono .brand-mark__dots { fill: currentColor; }
.brand-mark--brand .brand-mark__deck { stroke: var(--color-paper-dim); }
.brand-mark--brand .brand-mark__dots { fill: var(--color-crimson-400); }
.brand-mark__bowl-lit { stop-color: var(--color-sapphire-400); }
.brand-mark__bowl-deep { stop-color: var(--color-sapphire-700); }
</style>
