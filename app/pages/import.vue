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
  <div>
    <PageHeader title="وارد کردن فاکتورهای بارزپخش" description="فاکتورهای پرداخت‌شده خوانده می‌شود، هر قلم به قطعهٔ لندو وصل می‌شود و با قیمت روز روی قفسه می‌نشیند.">
      <template #actions>
        <UButton v-if="active" color="error" variant="soft" icon="i-lucide-square" @click="cancel">توقف</UButton>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <section v-if="!job.jobId.value || finished" class="space-y-4">
      <div class="grid gap-4 md:grid-cols-2">
        <label class="ticket" :data-selected="mode === 'QUANTITY'">
          <input v-model="mode" type="radio" name="mode" value="QUANTITY" class="sr-only" />
          <UIcon v-if="mode === 'QUANTITY'" name="i-lucide-check-circle-2" class="ticket-check size-5" />
          <span class="block text-lg font-semibold text-highlighted">با تعداد، از تاریخ</span>
          <span class="mt-1 block text-sm text-muted">تعداد خریداری‌شده در فاکتورهای پرداخت‌شده از این تاریخ به بعد، منهای مرجوعی‌ها، به قفسه اضافه می‌شود. اگر قطعه از قبل روی قفسه باشد به موجودی فعلی افزوده می‌شود. هر فاکتور فقط یک بار منتقل می‌شود.</span>
          <span class="mt-4 block text-sm font-medium text-highlighted">از تاریخ</span>
          <UInput v-model="since" aria-label="از تاریخ" placeholder="۱۴۰۵/۰۱/۰۱" dir="ltr" class="mt-1 w-40" :color="sinceOk ? 'primary' : 'error'" :disabled="mode !== 'QUANTITY'" />
        </label>
        <label class="ticket" :data-selected="mode === 'ZERO'">
          <input v-model="mode" type="radio" name="mode" value="ZERO" class="sr-only" />
          <UIcon v-if="mode === 'ZERO'" name="i-lucide-check-circle-2" class="ticket-check size-5" />
          <span class="block text-lg font-semibold text-highlighted">با تعداد صفر، برای بازبینی</span>
          <span class="mt-1 block text-sm text-muted">همهٔ اقلام همهٔ فاکتورهای پرداخت‌شده تا امروز با تعداد صفر و قیمت روز روی قفسه می‌آیند تا خودتان تعداد را اصلاح کنید. موجودی ردیف‌های موجود دست نمی‌خورد؛ فقط قیمت خریدشان به‌روز می‌شود.</span>
        </label>
      </div>
      <p class="max-w-prose text-sm text-muted">در هر دو حالت قیمت قفسه با قواعد قیمت‌گذاری خودتان در لندو از روی قیمت روز بارزپخش با ارزش افزوده محاسبه می‌شود، نه قیمت روی فاکتور.</p>
      <UButton size="lg" icon="i-lucide-play" :loading="starting" :disabled="mode === 'QUANTITY' && !sinceOk" @click="start">شروع وارد کردن</UButton>
    </section>

    <section v-if="job.jobId.value" class="surface mt-6">
      <div class="surface-head">
        <h2>عملیات <span class="tabular">{{ faNumber(job.jobId.value) }}</span></h2>
      </div>
      <div class="surface-body">
        <JobProgress kind="IMPORT" :phase="job.phase.value" :progress="job.progress.value" :events="job.events.value" :status="job.status.value" />
      </div>
    </section>

    <section v-if="job.status.value === 'REVIEW'" class="mt-6">
      <div class="mb-3 flex flex-wrap items-center justify-between gap-3">
        <h2 class="text-xl">بازبینی اقلام تطبیق‌نشده <span class="tabular text-muted">({{ faNumber(review.length) }})</span></h2>
        <UButton icon="i-lucide-arrow-left" trailing @click="cont">{{ review.length ? 'ادامه با اقلام تصمیم‌گرفته‌شده' : 'ادامه' }}</UButton>
      </div>
      <ReviewTable :items="review" :default-authenticity="settings?.default_authenticity ?? 'ORIGINAL'" @decided="decided" />
    </section>

    <section v-if="finished && job.detail.value" class="mt-6">
      <h2 class="mb-3 text-xl">گزارش</h2>
      <ReportTable :items="job.detail.value.items" kind="IMPORT" />
    </section>
  </div>
</template>
