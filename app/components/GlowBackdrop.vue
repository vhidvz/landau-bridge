<script setup lang="ts">
/** The lit night backdrop (spec §3.1). Static; the decorative layers are skipped under fx-lite. */
const reduced = useReducedEffects();
const noise =
  'data:image/svg+xml,' +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="160" height="160"><filter id="n"><feTurbulence type="fractalNoise" baseFrequency="0.82" numOctaves="3" stitchTiles="stitch"/></filter><rect width="100%" height="100%" filter="url(#n)"/></svg>`,
  );
</script>

<template>
  <div aria-hidden="true" class="pointer-events-none fixed inset-0 -z-10 overflow-hidden">
    <div class="absolute inset-0 bg-ink-950" />
    <template v-if="!reduced">
      <!-- crimson key light, outside the inline-start (top-right) corner -->
      <div class="absolute h-[70vh] w-[70vw] rounded-full opacity-[0.13]" style="top: -30%; inset-inline-start: -12%; background: radial-gradient(closest-side, #d63e3e, transparent 70%)" />
      <!-- sapphire counter-light, outside the inline-end (bottom-left) corner -->
      <div class="absolute h-[80vh] w-[70vw] rounded-full opacity-[0.10]" style="bottom: -35%; inset-inline-end: -18%; background: radial-gradient(closest-side, #3a3adb, transparent 70%)" />
      <!-- blueprint grid, fading down -->
      <div
        class="absolute inset-0"
        style="
          background-image:
            linear-gradient(to right, rgb(255 255 255 / 0.04) 1px, transparent 1px),
            linear-gradient(to bottom, rgb(255 255 255 / 0.04) 1px, transparent 1px),
            linear-gradient(to right, rgb(255 255 255 / 0.025) 1px, transparent 1px),
            linear-gradient(to bottom, rgb(255 255 255 / 0.025) 1px, transparent 1px);
          background-size: 88px 88px, 88px 88px, 22px 22px, 22px 22px;
          mask-image: radial-gradient(ellipse 90% 70% at 50% 0%, black 30%, transparent 75%);
        "
      />
      <!-- film grain (plain opacity, no blend mode: cheaper on a CPU renderer) -->
      <div class="absolute inset-0 opacity-[0.05]" :style="{ backgroundImage: `url(&quot;${noise}&quot;)` }" />
      <!-- vignette -->
      <div class="absolute inset-0" style="background: radial-gradient(ellipse 120% 90% at 50% 40%, transparent 55%, rgb(4 5 9 / 0.55) 100%)" />
    </template>
  </div>
</template>
