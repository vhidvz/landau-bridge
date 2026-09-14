<script setup lang="ts">
import type { BridgeError, SettingsView } from '~/types/bridge';

const toast = useToast();
const settings = ref<SettingsView | null>(null);
const error = ref<BridgeError | null>(null);
const starting = ref(false);
const overwrite = ref(false);
const job = useJob();

onMounted(async () => {
  try {
    settings.value = await bridge.settingsGet();
    overwrite.value = settings.value.overwrite_manual;
    const current = await bridge.jobCurrent();
    if (current?.kind === 'REFRESH') await job.follow(current.id);
  } catch (e) { error.value = e as BridgeError; }
});

const start = async () => {
  error.value = null; starting.value = true;
  try {
    if (settings.value && overwrite.value !== settings.value.overwrite_manual) {
      settings.value = await bridge.settingsSave({ overwrite_manual: overwrite.value });
    }
    const id = await bridge.refreshStart();
    await job.follow(id);
  } catch (e) { error.value = e as BridgeError; } finally { starting.value = false; }
};

const cancel = async () => {
  if (job.jobId.value == null) return;
  error.value = null;
  try {
    await bridge.jobCancel(job.jobId.value);
    toast.add({ title: 'درخواست توقف ثبت شد', color: 'neutral' });
    await job.reload();
  } catch (e) { error.value = e as BridgeError; }
};
const active = computed(() => job.status.value === 'RUNNING' || job.status.value === 'REVIEW');
const finished = computed(() => job.status.value && ['DONE', 'FAILED', 'CANCELLED', 'INTERRUPTED'].includes(job.status.value));
</script>

<template>
  <div>
    <PageHeader kicker="به‌روزرسانی قیمت‌ها" title="به‌روزرسانی قیمت‌های قفسه" lede="قیمت خرید هر ردیف از قیمت روز بارزپخش تازه می‌شود و قواعد قیمت‌گذاری لاندا دوباره اجرا می‌شود.">
      <template #actions>
        <UButton v-if="active" color="error" variant="soft" icon="i-lucide-square" @click="cancel">توقف</UButton>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <section v-if="!job.jobId.value || finished" class="surface">
      <div class="surface-body space-y-5">
        <p class="max-w-prose text-sm text-muted">برای هر ردیف قفسه که به یک کد بارزپخش وصل است، قیمت روز بارزپخش با ارزش افزوده به عنوان قیمت خرید ثبت می‌شود و سپس قواعد قیمت‌گذاری لاندا روی کل فروشگاه اجرا می‌شود. ردیف‌هایی که به بارزپخش وصل نیستند در گزارش فهرست می‌شوند و دست نمی‌خورند.</p>
        <UFormField label="قیمت‌های دستی لاندا را هم بازنویسی کن" hint="لاندا قیمتی را که خودتان در پنل تایپ کرده باشید حفظ می‌کند مگر این گزینه روشن باشد">
          <USwitch v-model="overwrite" />
        </UFormField>
        <UButton size="lg" icon="i-lucide-refresh-cw" :loading="starting" @click="start">شروع به‌روزرسانی</UButton>
      </div>
    </section>

    <section v-if="job.jobId.value" class="surface mt-6">
      <div class="surface-head">
        <h2>عملیات <span class="tabular">{{ faNumber(job.jobId.value) }}</span></h2>
      </div>
      <div class="surface-body">
        <JobProgress kind="REFRESH" :phase="job.phase.value" :progress="job.progress.value" :events="job.events.value" :status="job.status.value" />
      </div>
    </section>

    <section v-if="finished && job.detail.value" class="mt-6">
      <h2 class="mb-3 text-xl">گزارش</h2>
      <ReportTable :items="job.detail.value.items" kind="REFRESH" />
    </section>
  </div>
</template>
