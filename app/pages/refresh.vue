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
  <div v-reveal>
    <PageHeader kicker="به‌روزرسانی قیمت‌ها · ۰۳" title="قیمت‌ها، به روز." lede="قیمت خرید هر ردیف از قیمت روز بارزپخش تازه می‌شود و قواعد قیمت‌گذاری لاندا دوباره اجرا می‌شود.">
      <template v-if="active" #actions>
        <button type="button" class="btn-danger" @click="cancel"><UIcon name="i-lucide-square" class="size-4" />توقف</button>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <GlassCard v-if="!job.jobId.value || finished" data-reveal>
      <div class="space-y-5">
        <p class="max-w-[58ch] text-sm text-paper-dim">برای هر ردیف قفسه که به یک کد بارزپخش وصل است، قیمت روز بارزپخش با ارزش افزوده به عنوان قیمت خرید ثبت می‌شود و سپس قواعد قیمت‌گذاری لاندا روی کل فروشگاه اجرا می‌شود. ردیف‌هایی که به بارزپخش وصل نیستند در گزارش فهرست می‌شوند و دست نمی‌خورند.</p>
        <div class="flex items-center justify-between gap-6 border-t hairline pt-5">
          <div>
            <div class="text-sm text-paper">قیمت‌های دستی لاندا را هم بازنویسی کن</div>
            <p class="text-xs text-paper-mute">لاندا قیمتی را که خودتان در پنل تایپ کرده باشید حفظ می‌کند مگر این گزینه روشن باشد.</p>
          </div>
          <USwitch v-model="overwrite" aria-label="بازنویسی قیمت‌های دستی" />
        </div>
        <UButton size="lg" icon="i-lucide-refresh-cw" :loading="starting" @click="start">شروع به‌روزرسانی</UButton>
      </div>
    </GlassCard>

    <GlassCard v-if="job.jobId.value" :pad="false" class="mt-6" data-reveal>
      <div class="flex items-center justify-between border-b hairline px-6 py-4">
        <h2 class="display text-[1.0625rem] text-paper">عملیات <span class="tabular">{{ faNumber(job.jobId.value) }}</span></h2>
        <StatusPill v-if="job.status.value" :tone="statusTone(job.status.value)" :label="statusLabel[job.status.value] ?? job.status.value" />
      </div>
      <div class="px-6 py-5">
        <JobProgress kind="REFRESH" :phase="job.phase.value" :progress="job.progress.value" :events="job.events.value" :status="job.status.value" :show-status="false" />
      </div>
    </GlassCard>

    <section v-if="finished && job.detail.value" class="mt-6 space-y-3">
      <div v-if="summaryTiles(job.detail.value.job).length" class="grid grid-cols-3 gap-2">
        <StatTile v-for="t in summaryTiles(job.detail.value.job)" :key="t.label" :label="t.label" :value="t.value" />
      </div>
      <div class="flex items-center justify-between">
        <h2 class="display text-xl text-paper">گزارش</h2>
        <NuxtLink :to="`/jobs/${job.jobId.value}`" class="text-xs text-crimson-400 hover:text-crimson-300">برگهٔ عملیات ←</NuxtLink>
      </div>
      <ReportTable :items="job.detail.value.items" kind="REFRESH" />
    </section>
  </div>
</template>
