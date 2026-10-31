<script setup lang="ts">
import type { BridgeError, Dashboard, JobRow, SettingsView, UpdateView } from '~/types/bridge';

const toast = useToast();
const dash = ref<Dashboard | null>(null);
const settings = ref<SettingsView | null>(null);
const recent = ref<JobRow[]>([]);
const error = ref<BridgeError | null>(null);
const job = useJob();

const load = async () => {
  try {
    const d = await bridge.dashboardGet();
    settings.value = d.settings_complete ? null : await bridge.settingsGet();
    recent.value = d.settings_complete ? (await bridge.jobsList()).slice(0, 6) : [];
    dash.value = d;
    if (d.running && job.jobId.value !== d.running.id) await job.follow(d.running.id);
  } catch (e) { error.value = e as BridgeError; }
};
onMounted(load);

// A newer version waiting to go in: a band here, the details on Settings.
const upd = ref<UpdateView | null>(null);
const updInstalling = ref(false);
let offUpdate: (() => void) | null = null;
onMounted(async () => {
  upd.value = await bridge.updateStatus().catch(() => null);
  offUpdate = await onUpdateState((s) => { if (upd.value) upd.value = { ...upd.value, state: s }; });
});
onBeforeUnmount(() => offUpdate?.());
const installUpdate = async () => {
  error.value = null; updInstalling.value = true;
  try { await bridge.updateInstall(); } catch (e) {
    error.value = e as BridgeError;
    upd.value = await bridge.updateStatus().catch(() => upd.value);
  } finally { updInstalling.value = false; }
};
// the followed job finished: refresh the overview so «آخرین …» and the list catch up
watch(job.status, (s) => { if (s && !['RUNNING', 'REVIEW'].includes(s)) load(); });

const live = computed(() => (dash.value?.running ? { phase: job.phase.value, progress: job.progress.value, events: job.events.value, status: job.status.value } : null));
const runningTo = computed(() => (dash.value?.running ? (dash.value.running.kind === 'IMPORT' ? `/import?job=${dash.value.running.id}` : '/refresh') : '/'));
const inReview = computed(() => dash.value?.running?.status === 'REVIEW');
const supplierReady = computed(() => !!dash.value?.supplier_user);
const lede = computed(() => dash.value?.store_name
  ? `فاکتورهای بارزپخش خوانده می‌شود، هر قلم به قطعهٔ لاندا وصل می‌شود و با قواعد خودتان روی قفسهٔ «${dash.value.store_name}» می‌نشیند.`
  : 'فاکتورهای بارزپخش خوانده می‌شود، هر قلم به قطعهٔ لاندا وصل می‌شود و با قواعد خودتان روی قفسهٔ لاندا می‌نشیند.');

const lastRuns = computed(() => [
  { key: 'import', label: 'آخرین وارد کردن', job: dash.value?.last_import ?? null, to: '/import', action: 'وارد کردن فاکتورها' },
  { key: 'refresh', label: 'آخرین به‌روزرسانی قیمت', job: dash.value?.last_refresh ?? null, to: '/refresh', action: 'به‌روزرسانی قیمت‌ها' },
]);
const dotOf = (j: JobRow) => ({ ok: 'bg-ok', danger: 'bg-danger', warn: 'bg-warn', info: 'bg-warn', muted: 'bg-graphite-500', brand: 'bg-crimson-400' })[statusTone(j.status)];

const cancel = async () => {
  if (!dash.value?.running) return;
  error.value = null;
  try {
    await bridge.jobCancel(dash.value.running.id);
    toast.add({ title: 'درخواست توقف ثبت شد', color: 'neutral' });
  } catch (e) { error.value = e as BridgeError; }
};

// Linux AppImage only: offer to put the file in the applications menu (README, "Other Linux").
const installing = ref(false);
const installAppImage = async () => {
  error.value = null; installing.value = true;
  try {
    const path = await bridge.appimageInstall();
    toast.add({ title: 'در فهرست برنامه‌ها نصب شد', description: path, color: 'success' });
    await load();
  } catch (e) { error.value = e as BridgeError; } finally { installing.value = false; }
};
const dismissAppImage = async () => {
  try { await bridge.appimageDismiss(); await load(); } catch (e) { error.value = e as BridgeError; }
};
</script>

<template>
  <div v-reveal>
    <PageHeader kicker="داشبورد · ۰۱" title="دو کرانه، یک قفسه." :lede="lede">
      <template v-if="dash?.settings_complete" #actions>
        <NuxtLink v-if="dash.running" :to="runningTo" class="btn-ghost">رفتن به عملیات جاری</NuxtLink>
        <template v-else>
          <NuxtLink to="/refresh" class="btn-ghost"><UIcon name="i-lucide-refresh-cw" class="size-4" />به‌روزرسانی قیمت‌ها</NuxtLink>
          <NuxtLink to="/import" class="btn-primary"><UIcon name="i-lucide-file-input" class="size-4" />وارد کردن فاکتورها</NuxtLink>
        </template>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <template v-if="dash">
      <div v-if="upd && canInstallNow(upd)" class="glass-inset mb-6 flex flex-wrap items-center gap-x-4 gap-y-3 px-5 py-4" data-reveal>
        <UIcon name="i-lucide-circle-arrow-down" class="size-6 shrink-0 text-crimson-400" aria-hidden="true" />
        <div class="min-w-0 flex-1">
          <p class="font-medium text-paper">نسخهٔ تازهٔ پل لاندا آماده است</p>
          <p class="text-sm text-paper-mute">{{ updateLabel(upd.state, upd.kind).text }}</p>
        </div>
        <div class="flex gap-2">
          <UButton size="sm" icon="i-lucide-rotate-cw" :loading="updInstalling" :disabled="!!dash.running" @click="installUpdate">نصب و راه‌اندازی دوباره</UButton>
          <UButton size="sm" variant="outline" color="neutral" to="/settings">جزئیات</UButton>
        </div>
      </div>

      <div v-if="dash.appimage_offer" class="glass-inset mb-6 flex flex-wrap items-center gap-x-4 gap-y-3 px-5 py-4" data-reveal>
        <UIcon name="i-lucide-package-plus" class="size-6 shrink-0 text-crimson-400" aria-hidden="true" />
        <div class="min-w-0 flex-1">
          <p class="font-medium text-paper">برنامه از فایل AppImage اجرا شده است</p>
          <p class="text-sm text-paper-mute">می‌خواهید در فهرست برنامه‌ها نصب شود؟ نسخه‌ای از فایل در پوشهٔ خانگی شما (<code dir="ltr">~/.local/bin</code>) نگه داشته می‌شود و میان‌بر آن به منوی برنامه‌ها اضافه می‌شود.</p>
        </div>
        <div class="flex gap-2">
          <UButton size="sm" icon="i-lucide-download" :loading="installing" @click="installAppImage">نصب</UButton>
          <UButton size="sm" variant="outline" color="neutral" :disabled="installing" @click="dismissAppImage">نه، دیگر نپرس</UButton>
        </div>
      </div>

      <SetupChecklist v-if="!dash.settings_complete && settings" :settings="settings" data-reveal />

      <template v-else>
        <!-- status band -->
        <div class="mb-6 grid gap-4 md:grid-cols-4">
          <GlassCard class="hover-glow flex flex-col" data-reveal>
            <div class="mb-3 flex min-h-9 items-center justify-between gap-2"><span class="data-label">بارزپخش</span><StatusPill :tone="supplierReady ? 'ok' : 'warn'" :label="supplierReady ? 'آماده' : 'تنظیم‌نشده'" /></div>
            <div class="display tabular text-[1.75rem] leading-none text-paper">{{ faNumber(dash.invoices_cached) }}</div>
            <div class="mt-1.5 text-xs text-paper-mute">فاکتور ذخیره‌شده</div>
            <div class="mt-auto pt-3 text-xs" :class="dash.invoices_unpaid > 0 ? 'text-warn' : 'text-paper-mute'">{{ dash.invoices_unpaid > 0 ? `${faNumber(dash.invoices_unpaid)} پرداخت‌نشده` : 'همه پرداخت‌شده' }}</div>
            <div v-if="dash.supplier_user" class="truncate font-mono text-[0.6875rem] text-paper-mute" dir="ltr">{{ dash.supplier_user }}</div>
          </GlassCard>

          <GlassCard class="hover-glow flex flex-col" data-reveal>
            <div class="mb-3 flex min-h-9 items-center justify-between gap-2"><span class="data-label">لاندا</span><StatusPill :tone="dash.store_name ? 'ok' : 'warn'" :label="dash.store_name ? 'آماده' : 'تنظیم‌نشده'" /></div>
            <div class="display truncate text-[1.75rem] leading-none text-paper">{{ dash.store_name ?? '—' }}</div>
            <div class="mt-1.5 text-xs text-paper-mute tabular">{{ faNumber(dash.decided_matches) }} تطبیق ثبت‌شده</div>
            <NuxtLink to="/matches" class="mt-auto pt-3 text-xs text-crimson-400 hover:text-crimson-300">تطبیق‌ها ←</NuxtLink>
          </GlassCard>

          <GlassCard class="hover-glow flex flex-col" data-reveal>
            <div class="mb-3 flex min-h-9 items-center justify-between gap-2"><span class="data-label">در انتظار شما</span><StatusPill :tone="dash.pending_review > 0 ? 'warn' : 'ok'" :label="dash.pending_review > 0 ? 'نیاز به تصمیم' : 'چیزی نیست'" /></div>
            <div class="display tabular text-[1.75rem] leading-none text-paper">{{ faNumber(dash.pending_review) }}</div>
            <div class="mt-1.5 text-xs text-paper-mute">قلم منتظر تصمیم</div>
            <NuxtLink v-if="inReview" :to="runningTo" class="mt-auto pt-3 text-xs text-crimson-400 hover:text-crimson-300">ادامهٔ بازبینی ←</NuxtLink>
            <div v-else class="mt-auto pt-3 text-xs text-paper-mute">تصمیم‌ها در مرحلهٔ بازبینی پرسیده می‌شود</div>
          </GlassCard>

          <GlassCard class="hover-glow flex flex-col" data-reveal>
            <div class="mb-3 flex min-h-9 items-center justify-between gap-2"><span class="data-label">سلامت</span><StatusPill :tone="dash.recent_errors > 0 ? 'danger' : 'ok'" :label="dash.recent_errors > 0 ? 'بررسی کنید' : 'پاک'" /></div>
            <div class="display tabular text-[1.75rem] leading-none text-paper">{{ faNumber(dash.recent_errors) }}</div>
            <div class="mt-1.5 text-xs text-paper-mute">خطا و هشدار در هفت روز</div>
            <NuxtLink to="/errors" class="mt-auto pt-3 text-xs text-crimson-400 hover:text-crimson-300">خطاها ←</NuxtLink>
          </GlassCard>
        </div>

        <!-- the crossing -->
        <GlassCard :pad="false" class="mb-6 overflow-hidden" data-reveal>
          <BridgeBand />
          <div v-if="dash.running" class="space-y-5 px-6 py-5">
            <div class="flex flex-wrap items-center justify-between gap-3">
              <div>
                <p class="data-label">{{ inReview ? 'در انتظار تصمیم شما' : 'در حال اجرا' }}</p>
                <h2 class="display text-xl text-paper">عملیات <span class="tabular">{{ faNumber(dash.running.id) }}</span> · {{ jobKindLabel(dash.running) }}</h2>
              </div>
              <div class="flex gap-2">
                <NuxtLink :to="runningTo" class="btn-primary btn-sm">{{ inReview ? 'ادامهٔ بازبینی' : 'مشاهده' }}</NuxtLink>
                <button type="button" class="btn-danger btn-sm" @click="cancel">توقف</button>
              </div>
            </div>
            <JobProgress v-if="live" :kind="dash.running.kind" :phase="live.phase" :progress="live.progress" :events="live.events.slice(0, 3)" :status="live.status" />
          </div>
          <div v-else class="grid gap-6 px-6 py-5 md:grid-cols-2">
            <div v-for="r in lastRuns" :key="r.key" class="flex min-w-0 flex-col">
              <p class="data-label mb-2">{{ r.label }}</p>
              <template v-if="r.job">
                <div class="mb-3 flex flex-wrap items-center gap-2 text-xs text-paper-mute">
                  <StatusPill :tone="statusTone(r.job.status)" :label="statusLabel[r.job.status] ?? r.job.status" />
                  <span class="tabular">{{ isoToLocal(r.job.started_at) }}</span>
                  <span class="tabular">· {{ faDuration(r.job.started_at, r.job.finished_at) }}</span>
                </div>
                <div v-if="summaryTiles(r.job).length" class="grid grid-cols-3 gap-2">
                  <StatTile v-for="t in summaryTiles(r.job)" :key="t.label" :label="t.label" :value="t.value" :tone="t.label === 'ناموفق' && t.value > 0 ? 'danger' : undefined" />
                </div>
                <NuxtLink :to="`/jobs/${r.job.id}`" class="mt-auto pt-3 text-xs text-crimson-400 hover:text-crimson-300">برگهٔ عملیات ←</NuxtLink>
              </template>
              <template v-else>
                <p class="mb-3 text-sm text-paper-mute">هنوز اجرا نشده</p>
                <NuxtLink :to="r.to" class="btn-ghost btn-sm mt-auto self-start">{{ r.action }}</NuxtLink>
              </template>
            </div>
          </div>
        </GlassCard>

        <!-- recent activity -->
        <GlassCard :pad="false" data-reveal>
          <div class="flex items-center justify-between px-6 pt-5 pb-3">
            <h2 class="display text-xl text-paper">فعالیت اخیر</h2>
            <span class="text-xs text-paper-mute">{{ faNumber(recent.length) }} عملیات آخر</span>
          </div>
          <p v-if="!recent.length" class="px-6 pt-2 pb-8 text-sm text-paper-mute">هنوز عملیاتی اجرا نشده.</p>
          <ul v-else class="divide-y divide-white/5">
            <li v-for="j in recent" :key="j.id">
              <NuxtLink :to="`/jobs/${j.id}`" class="flex items-center gap-4 px-6 py-3 hover:bg-white/3">
                <span class="size-2 shrink-0 rounded-full" :class="dotOf(j)" aria-hidden="true" />
                <div class="min-w-0 flex-1">
                  <div class="text-[0.8125rem] text-paper">{{ jobKindLabel(j) }} <span v-if="j.since_date" class="text-xs text-paper-mute">از <bdi dir="ltr">{{ j.since_date }}</bdi></span></div>
                  <div class="truncate text-xs text-paper-mute">{{ summaryLabel(j) || (statusLabel[j.status] ?? j.status) }}</div>
                </div>
                <div class="shrink-0 text-end text-xs text-paper-mute tabular">
                  <div>{{ isoToLocal(j.started_at) }}</div>
                  <div>{{ faDuration(j.started_at, j.finished_at) }}</div>
                </div>
                <UIcon name="i-lucide-chevron-left" class="size-4 shrink-0 text-paper-mute" aria-hidden="true" />
              </NuxtLink>
            </li>
          </ul>
          <div class="border-t hairline px-6 py-3"><NuxtLink to="/jobs" class="text-xs text-crimson-400 hover:text-crimson-300">همهٔ عملیات‌ها ←</NuxtLink></div>
        </GlassCard>
      </template>
    </template>
  </div>
</template>
