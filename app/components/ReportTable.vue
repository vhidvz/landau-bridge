<script setup lang="ts">
import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import type { BridgeError, JobItemRow } from '~/types/bridge';

const props = defineProps<{ items: JobItemRow[]; kind: 'IMPORT' | 'REFRESH' }>();
const filter = ref<'ALL' | 'DONE' | 'CHANGED' | 'SKIPPED' | 'FAILED'>('ALL');
const query = ref('');
const rows = computed(() => props.items.filter((i) =>
  (filter.value === 'ALL' || (filter.value === 'CHANGED' ? priceDelta(i)?.dir === 'up' || priceDelta(i)?.dir === 'down' : i.status === filter.value))
  && (!query.value || i.name.includes(query.value) || i.code.includes(query.value) || (i.product_name ?? '').includes(query.value))));
const filters = [{ value: 'ALL', label: 'همه' }, { value: 'DONE', label: 'انجام‌شده' }, { value: 'CHANGED', label: 'تغییر قیمت' }, { value: 'SKIPPED', label: 'ردشده' }, { value: 'FAILED', label: 'ناموفق' }] as const;
const actionLabel: Record<string, string> = { ATTACH: 'افزوده شد', PATCH: 'به‌روزرسانی شد', SKIP: 'بدون تغییر' };
const error = ref<BridgeError | null>(null);
const saving = ref(false);

// A blob download is unreliable in WebKitGTK, so the export goes through the native save dialog
// and the fs plugin. The BOM makes Excel read the UTF-8 Persian text correctly.
const csv = async () => {
  const head = ['کد', 'نام', 'اقدام', 'تعداد قبل', 'تعداد بعد', 'قیمت خرید', 'قیمت قفسه قبل', 'قیمت قفسه بعد', 'تغییر قیمت', 'نتیجه قیمت‌گذاری', 'وضعیت', 'خطا'];
  const esc = (v: unknown) => `"${String(v ?? '').replace(/"/g, '""')}"`;
  const lines = [head.join(','), ...props.items.map((i) => [
    i.code, i.name, i.action ? actionLabel[i.action] : '—', i.qty_before, i.qty_after, i.cost, i.price_before, i.price_after,
    priceDeltaLabel(priceDelta(i)), verdictLabel(i.reprice_verdict), statusLabel[i.status] ?? i.status, i.error_msg,
  ].map(esc).join(','))];
  error.value = null;
  saving.value = true;
  try {
    const path = await save({
      defaultPath: `landau-bridge-${props.kind.toLowerCase()}.csv`,
      filters: [{ name: 'CSV', extensions: ['csv'] }],
    });
    if (path) await writeTextFile(path, '\ufeff' + lines.join('\n'));
  } catch (e) {
    error.value = isBridgeError(e)
      ? e
      : { kind: 'Other', message: 'ذخیره فایل CSV ناموفق بود.', detail: String(e) };
  } finally {
    saving.value = false;
  }
};
</script>

<template>
  <div class="space-y-3">
    <div class="flex flex-wrap items-center gap-3">
      <SegmentedControl v-model="filter" :items="filters" aria-label="فیلتر ردیف‌ها" />
      <span class="tabular text-xs text-paper-mute">{{ faNumber(rows.length) }} ردیف</span>
      <input v-model="query" class="field ms-auto w-64" placeholder="جست‌وجو در نام یا کد" aria-label="جست‌وجو در ردیف‌ها" />
      <UButton v-if="items.length" size="sm" color="neutral" variant="outline" icon="i-lucide-download" :loading="saving" @click="csv">خروجی CSV</UButton>
    </div>
    <ErrorBanner :error="error" />
    <LedgerTable v-if="rows.length" class="glass">
      <thead>
        <tr><th>قطعه</th><th>اقدام</th><th>تعداد</th><th>قیمت خرید</th><th>قیمت قفسه</th><th>تغییر قیمت</th><th>نتیجهٔ قیمت‌گذاری</th><th>وضعیت</th></tr>
      </thead>
      <tbody>
        <tr v-for="i in rows" :key="i.id">
          <td class="w-full max-w-0">
            <p class="truncate font-medium text-paper">{{ i.product_name ?? i.name }}</p>
            <p class="truncate text-xs text-paper-mute">{{ i.name }} <bdi dir="ltr" class="whitespace-nowrap">{{ i.code }}</bdi></p>
          </td>
          <td>{{ i.action ? actionLabel[i.action] : '—' }}</td>
          <td class="whitespace-nowrap">
            <span v-if="i.qty_before != null" class="block text-xs text-paper-mute">{{ faNumber(i.qty_before) }}</span>
            <span class="font-medium text-paper">{{ faNumber(i.qty_after ?? i.qty) }}</span>
          </td>
          <td class="whitespace-nowrap">{{ rials(i.cost) }}</td>
          <td class="whitespace-nowrap">
            <span v-if="i.price_before != null" class="block text-xs text-paper-mute">{{ faNumber(i.price_before) }}</span>
            <span class="font-medium text-paper">{{ faNumber(i.price_after) }}</span>
          </td>
          <td class="whitespace-nowrap" :class="priceDelta(i)?.dir === 'up' ? 'text-warn' : priceDelta(i)?.dir === 'down' ? 'text-info' : 'text-paper-mute'">{{ priceDeltaLabel(priceDelta(i)) }}</td>
          <td class="whitespace-nowrap">{{ verdictLabel(i.reprice_verdict) }}</td>
          <td>
            <StatusPill :tone="statusTone(i.status)" :label="statusLabel[i.status] ?? i.status" />
            <p v-if="i.error_msg" class="mt-1 text-xs text-danger">{{ kindLabel(i.error_kind ?? '') }}: {{ i.error_msg }}</p>
          </td>
        </tr>
      </tbody>
    </LedgerTable>
    <GlassCard v-else-if="items.length" :pad="false"><EmptyState art="shelf" title="ردیفی با این فیلتر نیست" description="فیلتر یا عبارت جست‌وجو را عوض کنید." /></GlassCard>
    <GlassCard v-else :pad="false"><EmptyState art="shelf" title="این عملیات ردیفی ندارد" /></GlassCard>
  </div>
</template>
