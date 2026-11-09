<script setup lang="ts">
// The money of one supplier item (plan 130 R-k): Barezpakhsh's list price today, the VAT applied,
// the cost the bridge would write from them, and the active rule Landau prices the row by — each
// figure with its unit and its source. A failed read shows as unknown, never as zero.
import type { Inspection } from '~/types/bridge';

const props = defineProps<{ inspection: Inspection | null; loading?: boolean; failed?: string | null }>();
const quote = computed(() => known(props.inspection?.quote) ?? null);
const quoteUnknown = computed(() => unknownText(props.inspection?.quote));
const rules = computed(() => known(props.inspection?.rules) ?? null);
const at = (iso: string) => new Intl.DateTimeFormat('fa-IR', { timeStyle: 'short' }).format(new Date(iso));
const vatLine = computed(() => {
  const q = quote.value;
  if (!q) return '';
  if (!q.has_vat) return 'بدون ارزش افزوده';
  const site = q.site_vat != null && Math.abs(q.site_vat - q.vat_setting) > 0.01 ? ` — سایت ${faPercent(q.site_vat, 2)} می‌گوید` : '';
  return `ارزش افزوده ${faPercent(q.vat_setting, 2)} (تنظیمات)${site}`;
});
</script>

<template>
  <div class="flex flex-wrap items-center gap-x-5 gap-y-1 text-xs text-paper-mute">
    <template v-if="loading && !inspection"><span>در حال خواندن قیمت روز بارزپخش…</span></template>
    <template v-else-if="failed && !inspection"><span class="text-warn">قیمت بارزپخش و قفسه نامعلوم: {{ failed }}</span></template>
    <template v-else-if="quote">
      <span :title="`فهرست بارزپخش، خوانده‌شده ساعت ${at(quote.read_at)}`">قیمت بارزپخش <span class="tabular text-paper">{{ money(quote.list_price) }}</span></span>
      <span>{{ vatLine }}</span>
      <span title="قیمت فهرست × (۱ + ارزش افزوده)، محاسبهٔ پل">قیمت خرید <span class="tabular text-paper">{{ money(quote.cost) }}</span></span>
      <StatusPill v-if="!quote.available" tone="warn" label="ناموجود؛ قیمت خرید فقط بالا می‌رود" />
      <span v-if="quote.renamed" class="text-warn">نام کالا در بارزپخش عوض شده؛ با کد پیدا شد</span>
    </template>
    <template v-else-if="inspection && !quoteUnknown"><span class="text-warn">بارزپخش امروز این نام و کد را ندارد؛ قیمت خرید نامعلوم است</span></template>
    <template v-else-if="quoteUnknown"><span class="text-warn">قیمت بارزپخش نامعلوم: {{ quoteUnknown }}</span></template>
    <span v-if="rules?.length" class="ms-auto" title="قیمت فروش را لاندا پس از نوشتن با قاعدهٔ شما می‌گذارد">قیمت فروش: {{ rules.map(ruleText).join('، ') }}</span>
  </div>
</template>
