<script setup lang="ts">
import type { BridgeError, Dashboard } from '~/types/bridge';

const dash = ref<Dashboard | null>(null);
const error = ref<BridgeError | null>(null);
const load = async () => { try { dash.value = await bridge.dashboardGet(); } catch (e) { error.value = e as BridgeError; } };
onMounted(load);
</script>

<template>
  <div class="space-y-6">
    <h2 class="text-2xl font-bold">داشبورد</h2>
    <ErrorBanner :error="error" />
    <UAlert v-if="dash && !dash.settings_complete" color="warning" variant="soft" title="تنظیمات ناقص است" description="کلید لندو، فروشگاه مقصد و ورود بارزپخش را در صفحه تنظیمات کامل کنید.">
      <template #actions><UButton to="/settings" size="sm">تنظیمات</UButton></template>
    </UAlert>

    <div v-if="dash" class="grid gap-4 md:grid-cols-3">
      <UCard><p class="text-sm text-neutral-500">فروشگاه لندو</p><p class="text-lg font-semibold">{{ dash.store_name ?? '—' }}</p></UCard>
      <UCard><p class="text-sm text-neutral-500">حساب بارزپخش</p><p class="text-lg font-semibold" dir="ltr">{{ dash.supplier_user ?? '—' }}</p></UCard>
      <UCard><p class="text-sm text-neutral-500">فاکتورهای ذخیره‌شده</p><p class="text-lg font-semibold">{{ faNumber(dash.invoices_cached) }} <span class="text-sm text-neutral-500">({{ faNumber(dash.invoices_unpaid) }} پرداخت‌نشده)</span></p></UCard>
      <UCard><p class="text-sm text-neutral-500">آخرین وارد کردن</p><p class="font-semibold">{{ dash.last_import ? `${isoToLocal(dash.last_import.started_at)} — ${statusLabel[dash.last_import.status] ?? dash.last_import.status}` : '—' }}</p></UCard>
      <UCard><p class="text-sm text-neutral-500">آخرین به‌روزرسانی قیمت</p><p class="font-semibold">{{ dash.last_refresh ? `${isoToLocal(dash.last_refresh.started_at)} — ${statusLabel[dash.last_refresh.status] ?? dash.last_refresh.status}` : '—' }}</p></UCard>
      <UCard><p class="text-sm text-neutral-500">تطبیق‌های ثبت‌شده</p><p class="text-lg font-semibold">{{ faNumber(dash.decided_matches) }}</p></UCard>
    </div>

    <UAlert v-if="dash?.running?.status === 'REVIEW'" color="info" variant="soft" :title="`${faNumber(dash.pending_review)} قلم منتظر تصمیم شماست`">
      <template #actions><UButton :to="`/import?job=${dash.running.id}`" size="sm">ادامه بازبینی</UButton></template>
    </UAlert>
    <UAlert v-if="dash && dash.recent_errors > 0" color="warning" variant="soft" :title="`${faNumber(dash.recent_errors)} خطا یا هشدار در هفت روز گذشته`">
      <template #actions><UButton to="/logs" size="sm" variant="soft">مشاهده</UButton></template>
    </UAlert>

    <div class="flex flex-wrap gap-4">
      <UButton size="xl" icon="i-lucide-file-input" to="/import" :disabled="!dash?.settings_complete || !!dash?.running">وارد کردن فاکتورها</UButton>
      <UButton size="xl" icon="i-lucide-refresh-cw" variant="soft" to="/refresh" :disabled="!dash?.settings_complete || !!dash?.running">به‌روزرسانی قیمت‌ها</UButton>
    </div>
  </div>
</template>
