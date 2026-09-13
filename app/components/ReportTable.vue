<script setup lang="ts">
import type { JobItemRow } from '~/types/bridge';

const props = defineProps<{ items: JobItemRow[]; kind: 'IMPORT' | 'REFRESH' }>();
const filter = ref<'ALL' | 'DONE' | 'SKIPPED' | 'FAILED'>('ALL');
const rows = computed(() => props.items.filter((i) => filter.value === 'ALL' || i.status === filter.value));
const actionLabel: Record<string, string> = { ATTACH: 'افزوده شد', PATCH: 'به‌روزرسانی شد', SKIP: 'بدون تغییر' };

const csv = () => {
  const head = ['code', 'name', 'action', 'qty_before', 'qty_after', 'cost', 'price_before', 'price_after', 'verdict', 'status', 'error'];
  const esc = (v: unknown) => `"${String(v ?? '').replace(/"/g, '""')}"`;
  const lines = [head.join(','), ...props.items.map((i) => [i.code, i.name, i.action, i.qty_before, i.qty_after, i.cost, i.price_before, i.price_after, i.reprice_verdict, i.status, i.error_msg].map(esc).join(','))];
  const blob = new Blob(['﻿' + lines.join('\n')], { type: 'text/csv;charset=utf-8' });
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = `landau-bridge-${props.kind.toLowerCase()}.csv`;
  a.click();
  URL.revokeObjectURL(a.href);
};
</script>

<template>
  <div class="space-y-3">
    <div class="flex flex-wrap items-center gap-2">
      <UButton v-for="f in (['ALL', 'DONE', 'SKIPPED', 'FAILED'] as const)" :key="f" size="sm" :variant="filter === f ? 'solid' : 'soft'" @click="filter = f">
        {{ { ALL: 'همه', DONE: 'انجام‌شده', SKIPPED: 'ردشده', FAILED: 'ناموفق' }[f] }}
      </UButton>
      <span class="ms-auto text-sm text-neutral-500">{{ faNumber(rows.length) }} ردیف</span>
      <UButton size="sm" variant="soft" icon="i-lucide-download" @click="csv">خروجی CSV</UButton>
    </div>
    <div class="overflow-x-auto rounded-lg border border-neutral-200 dark:border-neutral-800">
      <table class="w-full text-sm">
        <thead class="bg-neutral-50 dark:bg-neutral-900">
          <tr class="text-start">
            <th class="p-2 text-start">قطعه</th>
            <th class="p-2">اقدام</th>
            <th class="p-2">تعداد</th>
            <th class="p-2">قیمت خرید</th>
            <th class="p-2">قیمت قفسه</th>
            <th class="p-2">نتیجه قیمت‌گذاری</th>
            <th class="p-2">وضعیت</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="i in rows" :key="i.id" class="border-t border-neutral-200 dark:border-neutral-800">
            <td class="p-2"><p>{{ i.product_name ?? i.name }}</p><p class="text-xs text-neutral-500">{{ i.name }} <span dir="ltr">{{ i.code }}</span></p></td>
            <td class="p-2 text-center">{{ i.action ? actionLabel[i.action] : '—' }}</td>
            <td class="p-2 text-center" dir="ltr">{{ i.qty_before != null ? `${faNumber(i.qty_before)} → ` : '' }}{{ faNumber(i.qty_after ?? i.qty) }}</td>
            <td class="p-2 text-center">{{ rials(i.cost) }}</td>
            <td class="p-2 text-center" dir="ltr">{{ i.price_before != null ? `${faNumber(i.price_before)} → ` : '' }}{{ faNumber(i.price_after) }}</td>
            <td class="p-2 text-center">{{ verdictLabel(i.reprice_verdict) }}</td>
            <td class="p-2 text-center">
              <UBadge :color="i.status === 'DONE' ? 'success' : i.status === 'FAILED' ? 'error' : 'neutral'" variant="soft">{{ statusLabel[i.status] ?? i.status }}</UBadge>
              <p v-if="i.error_msg" class="mt-1 text-xs text-red-700">{{ kindLabel(i.error_kind ?? '') }}: {{ i.error_msg }}</p>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
