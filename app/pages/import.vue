<script setup lang="ts">
import type { BridgeError, Dashboard, JobItemRow, SettingsView } from '~/types/bridge';

const route = useRoute();
const toast = useToast();
const settings = ref<SettingsView | null>(null);
const dash = ref<Dashboard | null>(null);
const error = ref<BridgeError | null>(null);
const mode = ref<'QUANTITY' | 'ZERO'>('QUANTITY');
const since = ref(jalaliYearStart());
const sinceOnly = ref(true);
const starting = ref(false);
const review = ref<JobItemRow[]>([]);
const job = useJob();
const firstRun = computed(() => !!dash.value && dash.value.decided_matches === 0 && dash.value.last_import === null);

const sinceOk = computed(() => normalizeJalali(since.value) !== null);

const loadReview = async () => {
  if (job.jobId.value == null) return;
  try { review.value = await bridge.importReview(job.jobId.value); } catch (e) { error.value = e as BridgeError; }
};

onMounted(async () => {
  try {
    settings.value = await bridge.settingsGet();
    dash.value = await bridge.dashboardGet();
    since.value = nextSinceDate(dash.value.last_import);
    sinceOnly.value = settings.value.fetch_since_only;
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
    if (mode.value === 'QUANTITY' && settings.value && sinceOnly.value !== settings.value.fetch_since_only) {
      settings.value = await bridge.settingsSave({ fetch_since_only: sinceOnly.value });
    }
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
  <div v-reveal>
    <PageHeader kicker="وارد کردن فاکتورها · ۰۲" title="فاکتورها، روی قفسه." lede="فاکتورهای پرداخت‌شده خوانده می‌شود، هر قلم به قطعهٔ لاندا وصل می‌شود و با قیمت روز روی قفسه می‌نشیند.">
      <template v-if="active" #actions>
        <button type="button" class="btn-danger" @click="cancel"><UIcon name="i-lucide-square" class="size-4" />توقف</button>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <section v-if="!job.jobId.value || finished" class="space-y-4" data-reveal>
      <div v-if="firstRun" class="glass-inset flex flex-wrap items-center gap-x-6 gap-y-3 px-5 py-4">
        <div class="min-w-0 flex-1">
          <p class="data-label mb-1">مسیر پیشنهادی برای بار اول</p>
          <p class="text-sm text-paper-dim">۱. همهٔ فاکتورهای قدیمی را با تعداد صفر بیاورید و تطبیق‌ها را یک بار تصمیم بگیرید.</p>
          <p class="text-sm text-paper-dim">۲. از فردا، «با تعداد، از تاریخ» را اجرا کنید؛ تاریخ شروع خودش پیشنهاد می‌شود.</p>
        </div>
        <button type="button" class="btn-ghost btn-sm" @click="mode = 'ZERO'">انتخاب حالت بازبینی</button>
      </div>

      <div class="grid gap-4 md:grid-cols-2">
        <label class="glass ticket" :data-selected="mode === 'QUANTITY'">
          <input v-model="mode" type="radio" name="mode" value="QUANTITY" class="sr-only" />
          <UIcon v-if="mode === 'QUANTITY'" name="i-lucide-check-circle-2" class="ticket-check size-5" aria-hidden="true" />
          <span class="display block text-lg text-paper">با تعداد، از تاریخ</span>
          <span class="mt-1 block text-sm text-paper-mute">تعداد خریداری‌شده در فاکتورهای پرداخت‌شده از این تاریخ به بعد، منهای مرجوعی‌ها، به قفسه اضافه می‌شود. اگر قطعه از قبل روی قفسه باشد به موجودی فعلی افزوده می‌شود. هر فاکتور فقط یک بار منتقل می‌شود.</span>
          <span class="mt-4 block text-sm font-medium text-paper">از تاریخ</span>
          <input v-model="since" class="field mt-1 w-40" :class="!sinceOk && 'field-invalid'" aria-label="از تاریخ" placeholder="۱۴۰۵/۰۱/۰۱" dir="ltr" :disabled="mode !== 'QUANTITY'" />
          <span class="mt-3 flex items-center justify-between gap-4">
            <span class="text-sm text-paper">فقط فاکتورهای از این تاریخ از بارزپخش خوانده شود<span class="block text-xs text-paper-mute">خاموش: کل صورت‌حساب از ۱۳۹۰ خوانده و ذخیره می‌شود؛ کندتر، ولی فقط بار اول.</span></span>
            <USwitch v-model="sinceOnly" aria-label="فقط فاکتورهای از این تاریخ خوانده شود" :disabled="mode !== 'QUANTITY'" />
          </span>
        </label>
        <label class="glass ticket" :data-selected="mode === 'ZERO'">
          <input v-model="mode" type="radio" name="mode" value="ZERO" class="sr-only" />
          <UIcon v-if="mode === 'ZERO'" name="i-lucide-check-circle-2" class="ticket-check size-5" aria-hidden="true" />
          <span class="display block text-lg text-paper">با تعداد صفر، برای بازبینی</span>
          <span class="mt-1 block text-sm text-paper-mute">همهٔ اقلام همهٔ فاکتورهای پرداخت‌شده تا امروز با تعداد صفر و قیمت روز روی قفسه می‌آیند تا خودتان تعداد را اصلاح کنید. موجودی ردیف‌های موجود دست نمی‌خورد؛ فقط قیمت خریدشان به‌روز می‌شود.</span>
        </label>
      </div>
      <p class="max-w-[58ch] text-sm text-paper-mute">در هر دو حالت قیمت قفسه با قواعد قیمت‌گذاری خودتان در لاندا از روی قیمت روز بارزپخش با ارزش افزوده محاسبه می‌شود، نه قیمت روی فاکتور.</p>
      <UButton size="lg" icon="i-lucide-play" :loading="starting" :disabled="mode === 'QUANTITY' && !sinceOk" @click="start">شروع وارد کردن</UButton>
    </section>

    <GlassCard v-if="job.jobId.value" :pad="false" class="mt-6" data-reveal>
      <div class="flex items-center justify-between border-b hairline px-6 py-4">
        <h2 class="display text-[1.0625rem] text-paper">عملیات <span class="tabular">{{ faNumber(job.jobId.value) }}</span></h2>
        <StatusPill v-if="job.status.value" :tone="statusTone(job.status.value)" :label="statusLabel[job.status.value] ?? job.status.value" />
      </div>
      <div class="px-6 py-5">
        <JobProgress kind="IMPORT" :phase="job.phase.value" :progress="job.progress.value" :events="job.events.value" :status="job.status.value" :show-status="false" />
      </div>
    </GlassCard>

    <section v-if="job.status.value === 'REVIEW'" class="mt-6">
      <div class="mb-3 flex flex-wrap items-center justify-between gap-3">
        <h2 class="display text-xl text-paper">بازبینی اقلام تطبیق‌نشده <span class="tabular text-paper-mute">({{ faNumber(review.length) }})</span></h2>
        <UButton icon="i-lucide-arrow-left" trailing @click="cont">{{ review.length ? 'ادامه با اقلام تصمیم‌گرفته‌شده' : 'ادامه' }}</UButton>
      </div>
      <ReviewTable :items="review" :default-authenticity="settings?.default_authenticity ?? 'ORIGINAL'" @decided="decided" />
    </section>

    <section v-if="finished && job.detail.value" class="mt-6 space-y-3">
      <div v-if="summaryTiles(job.detail.value.job).length" class="grid grid-cols-3 gap-2 md:grid-cols-6">
        <StatTile v-for="t in summaryTiles(job.detail.value.job)" :key="t.label" :label="t.label" :value="t.value" :tone="t.label === 'ناموفق' && t.value > 0 ? 'danger' : undefined" />
      </div>
      <div class="flex items-center justify-between">
        <h2 class="display text-xl text-paper">گزارش</h2>
        <NuxtLink :to="`/jobs/${job.jobId.value}`" class="text-xs text-crimson-400 hover:text-crimson-300">برگهٔ عملیات ←</NuxtLink>
      </div>
      <ReportTable :items="job.detail.value.items" kind="IMPORT" />
    </section>
  </div>
</template>
