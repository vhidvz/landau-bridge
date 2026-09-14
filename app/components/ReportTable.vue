<script setup lang="ts">
import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import type { BridgeError, JobItemRow } from '~/types/bridge';

const props = defineProps<{ items: JobItemRow[]; kind: 'IMPORT' | 'REFRESH' }>();
const filter = ref<'ALL' | 'DONE' | 'SKIPPED' | 'FAILED'>('ALL');
const rows = computed(() => props.items.filter((i) => filter.value === 'ALL' || i.status === filter.value));
const filters = [{ value: 'ALL', label: 'همه' }, { value: 'DONE', label: 'انجام‌شده' }, { value: 'SKIPPED', label: 'ردشده' }, { value: 'FAILED', label: 'ناموفق' }] as const;
const actionLabel: Record<string, string> = { ATTACH: 'افزوده شد', PATCH: 'به‌روزرسانی شد', SKIP: 'بدون تغییر' };
const error = ref<BridgeError | null>(null);
const saving = ref(false);

// A blob download is unreliable in WebKitGTK, so the export goes through the native save dialog
// and the fs plugin. The BOM makes Excel read the UTF-8 Persian text correctly.
const csv = async () => {
  const head = ['کد', 'نام', 'اقدام', 'تعداد قبل', 'تعداد بعد', 'قیمت خرید', 'قیمت قفسه قبل', 'قیمت قفسه بعد', 'نتیجه قیمت‌گذاری', 'وضعیت', 'خطا'];
  const esc = (v: unknown) => `"${String(v ?? '').replace(/"/g, '""')}"`;
  const lines = [head.join(','), ...props.items.map((i) => [
    i.code, i.name, i.action ? actionLabel[i.action] : '—', i.qty_before, i.qty_after, i.cost, i.price_before, i.price_after,
    verdictLabel(i.reprice_verdict), statusLabel[i.status] ?? i.status, i.error_msg,
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
      <span class="tabular ms-auto text-sm text-muted">{{ faNumber(rows.length) }} ردیف</span>
      <UButton size="sm" variant="soft" icon="i-lucide-download" :loading="saving" @click="csv">خروجی CSV</UButton>
    </div>
    <ErrorBanner :error="error" />
    <LedgerTable v-if="rows.length">
      <thead>
        <tr>
          <th>قطعه</th>
          <th>اقدام</th>
          <th>تعداد</th>
          <th>قیمت خرید</th>
          <th>قیمت قفسه</th>
          <th>نتیجهٔ قیمت‌گذاری</th>
          <th>وضعیت</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="i in rows" :key="i.id">
          <td>
            <p class="font-medium text-highlighted">{{ i.product_name ?? i.name }}</p>
            <p class="text-xs text-muted">{{ i.name }} <span dir="ltr">{{ i.code }}</span></p>
          </td>
          <td>{{ i.action ? actionLabel[i.action] : '—' }}</td>
          <td>
            <span v-if="i.qty_before != null" class="block text-xs text-muted">{{ faNumber(i.qty_before) }}</span>
            <span class="font-medium">{{ faNumber(i.qty_after ?? i.qty) }}</span>
          </td>
          <td>{{ rials(i.cost) }}</td>
          <td>
            <span v-if="i.price_before != null" class="block text-xs text-muted">{{ faNumber(i.price_before) }}</span>
            <span class="font-medium">{{ faNumber(i.price_after) }}</span>
          </td>
          <td>{{ verdictLabel(i.reprice_verdict) }}</td>
          <td>
            <UBadge :color="i.status === 'DONE' ? 'success' : i.status === 'FAILED' ? 'error' : 'neutral'" variant="soft">{{ statusLabel[i.status] ?? i.status }}</UBadge>
            <p v-if="i.error_msg" class="mt-1 text-xs text-error">{{ kindLabel(i.error_kind ?? '') }}: {{ i.error_msg }}</p>
          </td>
        </tr>
      </tbody>
    </LedgerTable>
    <EmptyState v-else art="shelf" title="ردیفی با این فیلتر نیست" description="فیلتر دیگری را انتخاب کنید." />
  </div>
</template>
