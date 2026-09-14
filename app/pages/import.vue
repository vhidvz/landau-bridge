<script setup lang="ts">
import type { BridgeError, JobItemRow, SettingsView } from '~/types/bridge';

const route = useRoute();
const toast = useToast();
const settings = ref<SettingsView | null>(null);
const error = ref<BridgeError | null>(null);
const mode = ref<'QUANTITY' | 'ZERO'>('QUANTITY');
const since = ref(jalaliYearStart());
const starting = ref(false);
const review = ref<JobItemRow[]>([]);
const job = useJob();

const sinceOk = computed(() => normalizeJalali(since.value) !== null);

const loadReview = async () => {
  if (job.jobId.value == null) return;
  try { review.value = await bridge.importReview(job.jobId.value); } catch (e) { error.value = e as BridgeError; }
};

onMounted(async () => {
  try {
    settings.value = await bridge.settingsGet();
    const fromQuery = Number(route.query.job);
    const current = Number.isFinite(fromQuery) && fromQuery > 0 ? { id: fromQuery } : await bridge.jobCurrent();
    if (current) {
      await job.follow(current.id);
      if (job.status.value === 'REVIEW') await loadReview();
    }
  } catch (e) { error.value = e as BridgeError; }
});

watch(job.status, async (s) => { if (s === 'REVIEW') await loadReview(); });

const start = async () => {
  error.value = null; starting.value = true;
  try {
    const id = await bridge.importStart(mode.value, mode.value === 'QUANTITY' ? normalizeJalali(since.value)! : undefined);
    await job.follow(id);
  } catch (e) { error.value = e as BridgeError; } finally { starting.value = false; }
};

const decided = async (item_id: number) => {
  review.value = review.value.filter((i) => i.id !== item_id);
};

const cont = async () => {
  if (job.jobId.value == null) return;
  error.value = null;
  try {
    await bridge.importContinue(job.jobId.value);
    review.value = [];
    await job.reload();
  } catch (e) { error.value = e as BridgeError; }
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
  <div class="space-y-6">
    <h2 class="text-2xl font-bold">وارد کردن فاکتورهای بارزپخش</h2>
    <ErrorBanner :error="error" />

    <UCard v-if="!job.jobId.value || finished">
      <template #header><h3 class="font-semibold">انتخاب روش</h3></template>
      <div class="space-y-4">
        <label class="flex cursor-pointer items-start gap-3 rounded-lg border border-neutral-200 p-3 dark:border-neutral-800" :class="mode === 'QUANTITY' && 'ring-2 ring-primary-500'">
          <input v-model="mode" type="radio" name="mode" value="QUANTITY" class="mt-1" />
          <div>
            <p class="font-semibold">با تعداد، از تاریخ</p>
            <p class="text-sm text-neutral-600">تعداد خریداری‌شده در فاکتورهای پرداخت‌شده از این تاریخ به بعد (منهای مرجوعی‌ها) به قفسه اضافه می‌شود؛ اگر قطعه از قبل روی قفسه باشد، به موجودی فعلی افزوده می‌شود. هر فاکتور فقط یک بار منتقل می‌شود.</p>
            <UInput v-model="since" placeholder="۱۴۰۳/۰۱/۰۱" dir="ltr" class="mt-2 w-40" :color="sinceOk ? 'primary' : 'error'" />
          </div>
        </label>
        <label class="flex cursor-pointer items-start gap-3 rounded-lg border border-neutral-200 p-3 dark:border-neutral-800" :class="mode === 'ZERO' && 'ring-2 ring-primary-500'">
          <input v-model="mode" type="radio" name="mode" value="ZERO" class="mt-1" />
          <div>
            <p class="font-semibold">با تعداد صفر، برای بازبینی</p>
            <p class="text-sm text-neutral-600">همه اقلام همه فاکتورهای پرداخت‌شده تا امروز با تعداد صفر و قیمت روز روی قفسه می‌آیند تا خودتان تعداد را اصلاح کنید. موجودی ردیف‌های موجود دست نمی‌خورد؛ فقط قیمت خریدشان به‌روز می‌شود.</p>
          </div>
        </label>
        <p class="text-sm text-neutral-600">در هر دو حالت قیمت قفسه با قواعد قیمت‌گذاری خودتان در لندو از روی قیمت روز بارزپخش (با ارزش افزوده) محاسبه می‌شود، نه قیمت روی فاکتور.</p>
        <UButton size="lg" icon="i-lucide-play" :loading="starting" :disabled="mode === 'QUANTITY' && !sinceOk" @click="start">شروع</UButton>
      </div>
    </UCard>

    <UCard v-if="job.jobId.value">
      <template #header>
        <div class="flex items-center justify-between">
          <h3 class="font-semibold">عملیات شماره {{ faNumber(job.jobId.value) }}</h3>
          <UButton v-if="active" size="sm" color="error" variant="soft" icon="i-lucide-square" @click="cancel">توقف</UButton>
        </div>
      </template>
      <JobProgress kind="IMPORT" :phase="job.phase.value" :progress="job.progress.value" :events="job.events.value" :status="job.status.value" />
    </UCard>

    <UCard v-if="job.status.value === 'REVIEW'">
      <template #header>
        <div class="flex items-center justify-between">
          <h3 class="font-semibold">بازبینی اقلام تطبیق‌نشده ({{ faNumber(review.length) }})</h3>
          <UButton icon="i-lucide-arrow-left" @click="cont">{{ review.length ? 'ادامه با اقلام تصمیم‌گرفته‌شده' : 'ادامه' }}</UButton>
        </div>
      </template>
      <ReviewTable :items="review" :default-authenticity="settings?.default_authenticity ?? 'ORIGINAL'" @decided="decided" />
    </UCard>

    <UCard v-if="finished && job.detail.value">
      <template #header><h3 class="font-semibold">گزارش</h3></template>
      <ReportTable :items="job.detail.value.items" kind="IMPORT" />
    </UCard>
  </div>
</template>
