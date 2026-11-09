<script setup lang="ts">
import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import type { BridgeError, JobItemRow } from '~/types/bridge';

const props = defineProps<{ items: JobItemRow[]; kind: 'IMPORT' | 'REFRESH' }>();
type Filter = 'ALL' | 'DONE' | 'CHANGED' | 'SKIPPED' | 'FAILED';
const filter = ref<Filter>('ALL');
const query = ref('');
const changed = (i: JobItemRow) => { const d = priceDelta(i)?.dir; return d === 'up' || d === 'down'; };
const inFilter = (i: JobItemRow, f: Filter) => f === 'ALL' || (f === 'CHANGED' ? changed(i) : i.status === f);
const rows = computed(() => props.items.filter((i) =>
  inFilter(i, filter.value) && matchesQuery(query.value, [i.name, i.code, i.product_name, i.note, i.error_msg])));
// A refresh reports every shelf row (412 in the fixture, thousands on a big shelf): a page at a time.
const { page, rows: pageRows } = usePaged(rows, PAGE_SIZE.report, [filter, query]);
const filters = [
  { value: 'ALL', label: 'همه' }, { value: 'DONE', label: 'انجام‌شده' }, { value: 'CHANGED', label: 'تغییر قیمت' },
  { value: 'SKIPPED', label: 'ردشده' }, { value: 'FAILED', label: 'ناموفق' },
] as const;

// A verdict worth reading — the ordinary «priced» / «unchanged» is already said by the price itself.
const notableVerdict = (i: JobItemRow): string | null =>
  i.reprice_verdict && !i.reprice_verdict.startsWith('REPRICED:') && i.reprice_verdict !== 'UNCHANGED' ? verdictLabel(i.reprice_verdict) : null;
const qtyDelta = (i: JobItemRow): string | null => {
  const after = i.qty_after ?? i.qty;
  if (i.qty_before == null || after == null || after === i.qty_before) return null;
  const d = after - i.qty_before;
  return `${d > 0 ? '+' : '−'}${faNumber(Math.abs(d))}`;
};
const actionLabel: Record<string, string> = { ATTACH: 'افزوده شد', PATCH: 'به‌روزرسانی شد', SKIP: 'بدون تغییر' };
const error = ref<BridgeError | null>(null);
const saving = ref(false);

// A blob download is unreliable in WebKitGTK, so the export goes through the native save dialog
// and the fs plugin. The BOM makes Excel read the UTF-8 Persian text correctly.
const csv = async () => {
  const head = ['کد', 'نام', 'اقدام', 'تعداد قبل', 'تعداد بعد', 'قیمت خرید قبل', 'قیمت خرید', 'موجودی بارزپخش', 'قیمت قفسه قبل', 'قیمت قفسه بعد', 'تغییر قیمت', 'نتیجه قیمت‌گذاری', 'وضعیت', 'یادداشت', 'خطا'];
  const esc = (v: unknown) => `"${String(v ?? '').replace(/"/g, '""')}"`;
  const lines = [head.join(','), ...props.items.map((i) => [
    i.code, i.name, i.action ? actionLabel[i.action] : '—', i.qty_before, i.qty_after, i.cost_before, i.cost, availabilityLabel(i.available), i.price_before, i.price_after,
    priceDeltaLabel(priceDelta(i)), verdictLabel(i.reprice_verdict), statusLabel[i.status] ?? i.status, i.note, i.error_msg,
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
      <div class="ms-auto flex flex-wrap items-center gap-2">
        <div class="relative w-72">
          <input v-model="query" type="search" class="field pe-9" placeholder="جست‌وجو در نام یا کد" aria-label="جست‌وجو در ردیف‌ها" />
          <UIcon name="i-lucide-search" class="pointer-events-none absolute inset-y-0 end-3 my-auto size-4 text-paper-mute" aria-hidden="true" />
        </div>
        <UButton v-if="items.length" size="sm" color="neutral" variant="outline" icon="i-lucide-download" :loading="saving" @click="csv">خروجی CSV</UButton>
      </div>
    </div>
    <ErrorBanner :error="error" />
    <LedgerTable v-if="rows.length" class="glass">
      <thead>
        <tr><th>قطعه</th><th>اقدام</th><th>تعداد</th><th>قیمت خرید (ریال)</th><th>قیمت قفسه (ریال)</th><th>وضعیت</th></tr>
      </thead>
      <tbody>
        <tr v-for="i in pageRows" :key="i.id">
          <td class="w-full max-w-0">
            <p class="truncate font-medium text-paper" :title="i.product_name ?? i.name">{{ i.product_name ?? i.name }}</p>
            <p class="truncate text-xs text-paper-mute" :title="i.name">
              <bdi dir="ltr" class="whitespace-nowrap">{{ i.code }}</bdi><template v-if="i.product_name && i.product_name !== i.name"> · {{ i.name }}</template>
            </p>
            <p v-if="i.available != null || i.note || notableVerdict(i) || i.error_msg || i.error_kind" class="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-paper-mute">
              <StatusPill v-if="i.available != null" :tone="i.available ? 'ok' : 'warn'" :label="availabilityLabel(i.available)" />
              <span v-if="notableVerdict(i)">{{ notableVerdict(i) }}</span>
              <span v-if="i.note">{{ i.note }}</span>
              <span v-if="i.error_msg || i.error_kind" :class="i.status === 'FAILED' && 'text-danger'">{{ errorText(i.error_kind, i.error_msg) }}</span>
            </p>
          </td>
          <td class="whitespace-nowrap">{{ i.action ? actionLabel[i.action] : '—' }}</td>
          <td class="whitespace-nowrap">
            <span class="font-medium text-paper">{{ faNumber(i.qty_after ?? i.qty) }}</span>
            <span v-if="qtyDelta(i)" class="ms-1.5 text-xs text-paper-mute" :title="`پیش از این: ${faNumber(i.qty_before)}`"><bdi dir="ltr">{{ qtyDelta(i) }}</bdi></span>
          </td>
          <td class="whitespace-nowrap">
            <span>{{ faNumber(i.cost) }}</span>
            <span v-if="costBefore(i)" class="ms-1.5 text-xs text-paper-mute">{{ costBefore(i) }}</span>
          </td>
          <td class="whitespace-nowrap" :title="i.price_before != null && i.price_before !== i.price_after ? `پیش از این: ${faNumber(i.price_before)}` : undefined">
            <span class="font-medium text-paper">{{ faNumber(i.price_after) }}</span>
            <span v-if="priceDelta(i)?.dir === 'up' || priceDelta(i)?.dir === 'down'" class="ms-1.5 text-xs" :class="priceDelta(i)?.dir === 'up' ? 'text-warn' : 'text-info'">{{ priceDeltaLabel(priceDelta(i)) }}</span>
          </td>
          <td><StatusPill :tone="statusTone(i.status)" :label="statusLabel[i.status] ?? i.status" /></td>
        </tr>
      </tbody>
      <template #footer><Pager v-model:page="page" :total="rows.length" :items-per-page="PAGE_SIZE.report" /></template>
    </LedgerTable>
    <GlassCard v-else-if="items.length" :pad="false"><EmptyState art="shelf" title="ردیفی با این فیلتر نیست" description="فیلتر یا عبارت جست‌وجو را عوض کنید." /></GlassCard>
    <GlassCard v-else :pad="false"><EmptyState art="shelf" title="این عملیات ردیفی ندارد" /></GlassCard>
  </div>
</template>
