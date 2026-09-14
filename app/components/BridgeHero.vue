<script setup lang="ts">
import type { Dashboard } from '~/types/bridge';

defineProps<{ dash: Dashboard }>();
</script>

<template>
  <section class="surface relative overflow-hidden">
    <svg class="pointer-events-none absolute inset-x-0 top-0 h-full w-full" viewBox="0 0 1000 300" preserveAspectRatio="none" fill="none" aria-hidden="true">
      <!-- water -->
      <path d="M0 262q40-10 80 0t80 0 80 0 80 0 80 0 80 0 80 0 80 0 80 0 80 0 80 0 80 0" stroke="var(--color-firoozeh-200)" stroke-width="2" class="dark:[stroke:var(--color-firoozeh-900)]" />
      <path d="M0 280q40-8 80 0t80 0 80 0 80 0 80 0 80 0 80 0 80 0 80 0 80 0 80 0 80 0" stroke="var(--color-firoozeh-100)" stroke-width="2" class="dark:[stroke:var(--color-firoozeh-950)]" />
    </svg>
    <div class="relative grid gap-6 p-8 md:grid-cols-[1fr_1.4fr_1fr]">
      <!-- right bank (start side): Barezpakhsh -->
      <div class="min-w-0">
        <p class="text-sm font-medium text-muted">بارزپخش</p>
        <p class="tabular mt-1 text-[2.75rem] font-extrabold leading-none text-highlighted">{{ faNumber(dash.invoices_cached) }}</p>
        <p class="mt-1 text-sm">فاکتور ذخیره‌شده</p>
        <p v-if="dash.invoices_unpaid > 0" class="tabular text-sm text-muted">{{ faNumber(dash.invoices_unpaid) }} پرداخت‌نشده</p>
        <p v-if="dash.supplier_user" class="mt-3 truncate text-xs text-muted" dir="ltr">{{ dash.supplier_user }}</p>
      </div>

      <!-- the bridge -->
      <div class="flex min-w-0 flex-col items-center justify-end gap-4">
        <svg class="w-full max-w-[420px]" viewBox="0 0 420 120" fill="none" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M10 78H410" stroke="var(--color-firoozeh-600)" stroke-width="4" pathLength="1" class="draw motion-safe:[animation-delay:0ms]" />
          <path d="M90 78V26M330 78V26" stroke="var(--color-firoozeh-600)" stroke-width="4" pathLength="1" class="draw motion-safe:[animation-delay:150ms]" />
          <path d="M10 70Q50 30 90 26Q210 90 330 26Q370 30 410 70" stroke="var(--color-zaferan-400)" stroke-width="3" pathLength="1" class="draw motion-safe:[animation-delay:300ms]" />
          <path d="M120 44V78M150 55V78M180 63V78M210 66V78M240 63V78M270 55V78M300 44V78" stroke="var(--color-firoozeh-300)" stroke-width="2" pathLength="1" class="draw motion-safe:[animation-delay:600ms]" />
        </svg>
        <RunningJobCard v-if="dash.running" :job="dash.running" class="w-full !bg-firoozeh-600 !text-white hover:!bg-firoozeh-700" />
        <div v-else class="flex flex-wrap justify-center gap-3">
          <UButton size="lg" icon="i-lucide-file-input" to="/import">وارد کردن فاکتورها</UButton>
          <UButton size="lg" icon="i-lucide-refresh-cw" variant="soft" to="/refresh">به‌روزرسانی قیمت‌ها</UButton>
        </div>
      </div>

      <!-- left bank (end side): Landau -->
      <div class="min-w-0 md:text-end">
        <p class="text-sm font-medium text-muted">لاندا</p>
        <p class="mt-1 truncate text-2xl font-bold leading-tight text-highlighted">{{ dash.store_name ?? '—' }}</p>
        <p class="tabular mt-3 text-[2rem] font-extrabold leading-none text-highlighted">{{ faNumber(dash.decided_matches) }}</p>
        <p class="mt-1 text-sm">تطبیق ثبت‌شده</p>
      </div>
    </div>
  </section>
</template>
