<script setup lang="ts">
// What the names say about one pick (plan 130 R-k): numbers, words and brand, each with its state
// and the values that decided it — so a seller sees why a pair is consistent or not.
import type { Evidence } from '~/types/bridge';

const props = defineProps<{ evidence: Evidence }>();

const numbers = computed(() => {
  const n = props.evidence.numbers;
  const parts: string[] = [];
  if (n.shared.length) parts.push(`مشترک: ${n.shared.map(numLabel).join('، ')}`);
  if (n.lacking.length) parts.push(`فقط در قطعه: ${n.lacking.map(numLabel).join('، ')}`);
  if (!parts.length && n.supplier.length) parts.push(`بارزپخش: ${n.supplier.map(numLabel).join('، ')}`);
  return parts.join(' · ') || 'عددی در نام‌ها نیست';
});
const words = computed(() => {
  const w = props.evidence.words;
  const parts: string[] = [];
  for (const [s, c] of w.opposites) parts.push(`«${s}» در برابر «${c}»`);
  if (w.uncovered.length) parts.push(`در نام بارزپخش نیست: ${w.uncovered.join('، ')}`);
  if (w.missing.length) parts.push(`در قطعه نیست: ${w.missing.join('، ')}`);
  return parts.join(' · ') || 'همهٔ واژه‌های قطعه در نام بارزپخش هست';
});
const brand = computed(() => {
  const b = props.evidence.brand;
  const supplier = b.supplier.kind === 'ONE' ? b.supplier.brand : b.supplier.kind === 'SEVERAL' ? b.supplier.brands.join('، ') : '—';
  return `بارزپخش: ${supplier} · قطعه: ${b.candidate ?? '—'}`;
});
const rows = computed(() => [
  { label: 'عددها', state: props.evidence.numbers.state, text: numbers.value },
  { label: 'واژه‌ها', state: props.evidence.words.state, text: words.value },
  { label: 'برند', state: props.evidence.brand.state, text: brand.value },
]);
</script>

<template>
  <dl class="grid grid-cols-[4.5rem_5.5rem_1fr] items-center gap-x-3 gap-y-1.5 text-xs">
    <template v-for="r in rows" :key="r.label">
      <dt class="text-paper-mute">{{ r.label }}</dt>
      <dd><StatusPill :tone="stateTone[r.state]" :label="stateText[r.state]" /></dd>
      <dd class="min-w-0 truncate text-paper-dim" :title="r.text">{{ r.text }}</dd>
    </template>
  </dl>
</template>
