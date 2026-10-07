<script setup lang="ts">
import { documentDir, homeDir, join } from '@tauri-apps/api/path';
import { open, save } from '@tauri-apps/plugin-dialog';
import type { BridgeError, DecisionImportReport, MatchRow, Scored, UndecidedRow } from '~/types/bridge';

const rows = ref<MatchRow[]>([]);
const undecided = ref<UndecidedRow[]>([]);
const error = ref<BridgeError | null>(null);
const filter = ref<'ALL' | 'AUTO' | 'USER' | 'MAP' | 'NONE' | 'UNDECIDED'>('ALL');
const query = ref('');
const editing = ref<MatchRow | null>(null);
const deciding = ref<UndecidedRow | null>(null);
const term = ref('');
const candidates = ref<Scored[]>([]);
const searching = ref(false);
const toast = useToast();

const load = async () => {
  try { [rows.value, undecided.value] = await Promise.all([bridge.matchesList(), bridge.matchesUndecided()]); } catch (e) { error.value = e as BridgeError; }
};
onMounted(load);

const has = (s: string | null | undefined) => !query.value || (s ?? '').includes(query.value);
const shown = computed(() => rows.value.filter((r) => (filter.value === 'ALL' || r.how === filter.value) && (has(r.name_seen) || has(r.code) || has(r.product_name))));
const shownUndecided = computed(() => undecided.value.filter((u) => has(u.name_seen) || has(u.code) || u.candidates.some((c) => has(c.candidate.name))));
const PAGE = 50;
const { page: matchPage, rows: matchRows } = usePaged(shown, PAGE, [filter, query]);
const { page: undecidedPage, rows: undecidedRows, showIndex: showUndecided } = usePaged(shownUndecided, PAGE, [filter, query]);
const howLabel: Record<string, string> = { AUTO: 'خودکار', USER: 'انتخاب شما', MAP: 'از نگاشت', NONE: 'در لاندا نیست' };
const filters = computed(() => [
  { value: 'ALL', label: 'همه' }, { value: 'AUTO', label: 'خودکار' }, { value: 'USER', label: 'انتخاب شما' }, { value: 'MAP', label: 'از نگاشت' }, { value: 'NONE', label: 'در لاندا نیست' },
  { value: 'UNDECIDED', label: undecided.value.length ? `بدون تصمیم · ${faNumber(undecided.value.length)}` : 'بدون تصمیم' },
] as const);

const openEdit = (r: MatchRow) => { editing.value = r; term.value = r.name_seen; candidates.value = []; };
// An undecided part opens on the candidates review showed — no new search; one is there if wanted.
const openDecide = (u: UndecidedRow | null) => { deciding.value = u; term.value = u?.name_seen ?? ''; candidates.value = u?.candidates ?? []; };
const search = async () => {
  searching.value = true;
  try { candidates.value = await bridge.reviewSearch(term.value); } catch (e) { error.value = e as BridgeError; } finally { searching.value = false; }
};
const decided = async (u: UndecidedRow) => {
  const next = nextAfterDecision(shownUndecided.value, u.name_key);
  undecided.value = undecided.value.filter((x) => x.name_key !== u.name_key);
  openDecide(next);
  rows.value = await bridge.matchesList();
};
const pick = async (c: Scored) => {
  const target = deciding.value ?? editing.value;
  if (!target) return;
  error.value = null;
  try {
    await bridge.matchSet(target.name_seen, target.code, c.candidate.id, c.candidate.name, c.similarity);
    if (deciding.value) await decided(deciding.value); else { editing.value = null; await load(); }
  } catch (e) { error.value = e as BridgeError; }
};
const setNone = async () => {
  const target = deciding.value ?? editing.value;
  if (!target) return;
  error.value = null;
  try {
    await bridge.matchNone(target.name_seen, target.code);
    if (deciding.value) await decided(deciding.value); else { editing.value = null; await load(); }
  } catch (e) { error.value = e as BridgeError; }
};
const later = () => { if (deciding.value) openDecide(nextAfterSkip(shownUndecided.value, deciding.value.name_key)); };
const clear = async () => {
  if (!editing.value) return;
  error.value = null;
  try {
    await bridge.matchClear(editing.value.name_seen);
    editing.value = null; await load();
  } catch (e) { error.value = e as BridgeError; }
};
const position = computed(() => (deciding.value ? shownUndecided.value.findIndex((u) => u.name_key === deciding.value!.name_key) + 1 : 0));
// The page behind the dialog follows the part being decided, so closing it lands where you were.
watch(position, (p) => { if (p > 0) showUndecided(p - 1); });

// ---- moving decisions between installations. The files are read and written by the Rust side
// from the path the native dialog answers; in `pnpm dev` without Tauri a placeholder path stands in.
const inTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
// Given a bare file name, GTK opens the dialog in the process's working directory — for the
// AppImage its read-only mount (`/tmp/.mount_…/usr`, seen in the installed app 2026-10-07), where a
// save fails. Start in the seller's Documents, as the fs scope already expects.
const startDir = async () => { try { return await documentDir(); } catch { return await homeDir(); } };
const asPath = (p: unknown) => (typeof p === 'string' ? p : (p as { path: string } | null)?.path ?? null);
const busy = ref<'export' | 'import' | null>(null);
const importing = ref<string | null>(null);
const preferFile = ref(false);
const report = ref<DecisionImportReport | null>(null);

const exportDecisions = async () => {
  error.value = null;
  try {
    const path = inTauri()
      ? asPath(await save({ defaultPath: await join(await startDir(), `landau-bridge-decisions-${new Date().toISOString().slice(0, 10)}.json`), filters: [{ name: 'JSON', extensions: ['json'] }] }))
      : 'dev://decisions.json';
    if (!path) return;
    busy.value = 'export';
    const n = await bridge.decisionsExport(path);
    toast.add({ title: `${faNumber(n)} تصمیم در فایل ذخیره شد`, color: 'success' });
  } catch (e) { error.value = e as BridgeError; } finally { busy.value = null; }
};
const chooseImport = async () => {
  error.value = null;
  try {
    const path = inTauri() ? asPath(await open({ defaultPath: await startDir(), multiple: false, directory: false, filters: [{ name: 'JSON', extensions: ['json'] }] })) : 'dev://decisions.json';
    if (path) { importing.value = path; preferFile.value = false; report.value = null; }
  } catch (e) { error.value = e as BridgeError; }
};
const runImport = async () => {
  if (!importing.value) return;
  error.value = null; busy.value = 'import';
  try { report.value = await bridge.decisionsImport(importing.value, preferFile.value); await load(); } catch (e) { error.value = e as BridgeError; importing.value = null; } finally { busy.value = null; }
};
const closeImport = () => { importing.value = null; report.value = null; };
</script>

<template>
  <div v-reveal>
    <PageHeader kicker="تطبیق‌ها · ۰۴" title="هر کد، یک قطعه." lede="هر کد بارزپخش یک بار به یک قطعهٔ لاندا وصل می‌شود و دیگر پرسیده نمی‌شود." />
    <ErrorBanner :error="error" />

    <div class="mb-4 flex flex-wrap items-center gap-3" data-reveal>
      <SegmentedControl v-model="filter" :items="filters" aria-label="نوع تطبیق" />
      <span class="tabular text-xs text-paper-mute">{{ filter === 'UNDECIDED' ? `${faNumber(shownUndecided.length)} قطعه بدون تصمیم` : `${faNumber(shown.length)} تطبیق` }}</span>
      <div class="ms-auto flex flex-wrap items-center gap-2">
        <button type="button" class="btn-ghost btn-sm" :disabled="!!busy" @click="exportDecisions"><UIcon name="i-lucide-download" class="size-4" />ذخیرهٔ تصمیم‌ها</button>
        <button type="button" class="btn-ghost btn-sm" :disabled="!!busy" @click="chooseImport"><UIcon name="i-lucide-upload" class="size-4" />بازیابی تصمیم‌ها</button>
        <div class="relative w-72">
          <input v-model="query" class="field pe-9" placeholder="جست‌وجو در نام یا کد" aria-label="جست‌وجو در تطبیق‌ها" />
          <UIcon name="i-lucide-search" class="pointer-events-none absolute inset-y-0 end-3 my-auto size-4 text-paper-mute" aria-hidden="true" />
        </div>
      </div>
    </div>

    <template v-if="filter === 'UNDECIDED'">
      <LedgerTable v-if="shownUndecided.length" class="glass" data-reveal>
        <thead>
          <tr><th>نام در فاکتور</th><th>کد</th><th>نزدیک‌ترین پیشنهاد</th><th>شباهت</th><th>دفعات</th><th>آخرین بار</th><th></th></tr>
        </thead>
        <tbody>
          <tr v-for="u in undecidedRows" :key="u.name_key">
            <td class="font-medium text-paper">{{ u.name_seen }}</td>
            <td><bdi dir="ltr" class="whitespace-nowrap">{{ u.code }}</bdi></td>
            <td>{{ u.candidates[0]?.candidate.name ?? '—' }}</td>
            <td><SimilarityBar v-if="u.candidates[0]" :value="u.candidates[0].similarity" /><span v-else class="text-paper-mute">—</span></td>
            <td class="tabular">{{ faNumber(u.times) }}</td>
            <td class="whitespace-nowrap">{{ isoToLocal(u.seen_at) }}</td>
            <td class="text-end"><button type="button" class="btn-primary btn-sm" @click="openDecide(u)">تصمیم</button></td>
          </tr>
        </tbody>
      </LedgerTable>
      <Pager v-if="shownUndecided.length > PAGE" v-model:page="undecidedPage" :total="shownUndecided.length" :items-per-page="PAGE" class="mt-4 flex justify-center" />
      <GlassCard v-if="!shownUndecided.length" :pad="false" data-reveal>
        <EmptyState
          :title="undecided.length ? 'قطعه‌ای با این عبارت نیست' : 'قطعهٔ بدون تصمیمی نمانده'"
          :description="undecided.length ? 'عبارت جست‌وجو را عوض کنید.' : 'قطعه‌هایی که در بازبینی «بعداً» خورده‌اند و عملیات بدون آن‌ها ادامه یافته، با همان پیشنهادهای بازبینی اینجا می‌مانند تا هر وقت خواستید درباره‌شان تصمیم بگیرید.'"
        />
      </GlassCard>
    </template>
    <template v-else>
      <LedgerTable v-if="shown.length" class="glass" data-reveal>
        <thead>
          <tr><th>نام در فاکتور</th><th>کد</th><th>قطعهٔ لاندا</th><th>نحوه</th><th>شباهت</th><th>تاریخ</th><th></th></tr>
        </thead>
        <tbody>
          <tr v-for="r in matchRows" :key="r.name_key">
            <td class="font-medium text-paper">{{ r.name_seen }}</td>
            <td><bdi dir="ltr" class="whitespace-nowrap">{{ r.code }}</bdi></td>
            <td>{{ r.product_name ?? '—' }}</td>
            <td class="whitespace-nowrap"><StatusPill :tone="r.how === 'NONE' ? 'danger' : r.how === 'USER' ? 'ok' : 'info'" :label="howLabel[r.how] ?? r.how" /></td>
            <td><SimilarityBar v-if="r.similarity != null" :value="r.similarity" /><span v-else class="text-paper-mute">—</span></td>
            <td class="whitespace-nowrap">{{ isoToLocal(r.decided_at) }}</td>
            <td class="text-end"><button type="button" class="btn-ghost btn-sm" @click="openEdit(r)">تغییر</button></td>
          </tr>
        </tbody>
      </LedgerTable>
      <Pager v-if="shown.length > PAGE" v-model:page="matchPage" :total="shown.length" :items-per-page="PAGE" class="mt-4 flex justify-center" />
      <GlassCard v-if="!shown.length" :pad="false" data-reveal>
        <EmptyState
          :title="rows.length ? 'تطبیقی با این فیلتر نیست' : 'هنوز تطبیقی ثبت نشده'"
          :description="rows.length ? 'فیلتر یا عبارت جست‌وجو را عوض کنید.' : 'با اولین وارد کردن فاکتورها، تطبیق‌های خودکار و تصمیم‌های شما اینجا جمع می‌شود. برای اقلامی که «در لاندا نیست» علامت خورده‌اند، تیکتی برای پشتیبانی لاندا ثبت کنید و بعد تصمیم را همین‌جا تغییر دهید.'"
        >
          <template v-if="!rows.length" #action><NuxtLink to="/import" class="btn-ghost"><UIcon name="i-lucide-file-input" class="size-4" />وارد کردن فاکتورها</NuxtLink></template>
        </EmptyState>
      </GlassCard>
    </template>

    <UModal :open="!!editing || !!deciding" :title="deciding ? 'تصمیم برای قطعه' : 'تغییر تطبیق'" @update:open="(v: boolean) => { if (!v) { editing = null; deciding = null; } }">
      <template #body>
        <div v-if="editing || deciding" class="space-y-4">
          <p><span class="font-medium text-paper">{{ (deciding ?? editing)!.name_seen }}</span> <bdi dir="ltr" class="whitespace-nowrap text-sm text-paper-mute">{{ (deciding ?? editing)!.code }}</bdi></p>
          <p v-if="deciding" class="text-xs text-paper-mute">
            {{ faNumber(position) }} از {{ faNumber(shownUndecided.length) }} · {{ faNumber(deciding.times) }} بار بدون تصمیم ماند، آخرین بار {{ isoToLocal(deciding.seen_at) }}. پیشنهادهای همان بازبینی:
          </p>
          <div class="flex gap-2">
            <input v-model="term" class="field flex-1" aria-label="عبارت جست‌وجو" @keyup.enter="search" />
            <UButton :loading="searching" icon="i-lucide-search" @click="search">جست‌وجو در لاندا</UButton>
          </div>
          <p v-if="deciding && !candidates.length" class="text-sm text-paper-mute">لاندا در آن بازبینی پیشنهادی نداشت. عبارت دیگری را جست‌وجو کنید یا «در لاندا نیست» را بزنید.</p>
          <ul class="max-h-72 divide-y divide-white/5 overflow-auto">
            <li v-for="c in candidates" :key="c.candidate.id" class="flex items-center justify-between gap-3 py-2.5">
              <span class="min-w-0 truncate text-paper">{{ c.candidate.name }} <span v-if="c.candidate.brand" class="text-xs text-paper-mute">{{ c.candidate.brand }}</span></span>
              <span class="flex shrink-0 items-center gap-3"><SimilarityBar :value="c.similarity" /><button type="button" class="btn-primary btn-sm" @click="pick(c)">انتخاب</button></span>
            </li>
          </ul>
          <div class="flex flex-wrap justify-between gap-2 border-t hairline pt-4">
            <button type="button" class="btn-danger" @click="setNone">در لاندا نیست</button>
            <button v-if="deciding" type="button" class="btn-ghost" @click="later">بعداً</button>
            <button v-else type="button" class="btn-ghost" @click="clear">حذف تصمیم، دوباره پرسیده شود</button>
          </div>
        </div>
      </template>
    </UModal>

    <UModal :open="!!importing" title="بازیابی تصمیم‌ها" @update:open="(v: boolean) => { if (!v && busy !== 'import') closeImport(); }">
      <template #body>
        <div v-if="!report" class="space-y-4">
          <p class="text-sm">تصمیم‌های فایل با تصمیم‌های این برنامه ادغام می‌شوند. تصمیمی که شما گرفته‌اید (انتخاب یا «در لاندا نیست») بر تطبیق خودکار و نگاشت مقدم است. محصول هر تصمیم پیش از ثبت در فهرست فعلی لاندا بررسی می‌شود و اگر دیگر نباشد ثبت نمی‌شود.</p>
          <label class="flex items-center gap-2 text-sm"><input v-model="preferFile" type="checkbox" />اگر هر دو طرف تصمیمِ هم‌ارز و متفاوتی دارند، تصمیم فایل جایگزین شود</label>
          <div class="flex justify-end gap-2 border-t hairline pt-4">
            <button type="button" class="btn-ghost" :disabled="busy === 'import'" @click="closeImport">انصراف</button>
            <UButton :loading="busy === 'import'" icon="i-lucide-upload" @click="runImport">بازیابی</UButton>
          </div>
        </div>
        <div v-else class="space-y-4">
          <dl class="grid grid-cols-2 gap-x-6 gap-y-1 text-sm sm:grid-cols-4">
            <div><dt class="text-paper-mute">در فایل</dt><dd class="tabular">{{ faNumber(report.total) }}</dd></div>
            <div><dt class="text-paper-mute">افزوده شد</dt><dd class="tabular">{{ faNumber(report.added) }}</dd></div>
            <div><dt class="text-paper-mute">جایگزین شد</dt><dd class="tabular">{{ faNumber(report.replaced) }}</dd></div>
            <div><dt class="text-paper-mute">یکسان بود</dt><dd class="tabular">{{ faNumber(report.unchanged) }}</dd></div>
            <div><dt class="text-paper-mute">تصمیم این برنامه ماند</dt><dd class="tabular">{{ faNumber(report.kept.length) }}</dd></div>
            <div><dt class="text-paper-mute">محصول در لاندا نیست</dt><dd class="tabular">{{ faNumber(report.missing.length) }}</dd></div>
            <div><dt class="text-paper-mute">نامعتبر</dt><dd class="tabular">{{ faNumber(report.invalid) }}</dd></div>
          </dl>
          <div v-if="report.kept.length">
            <p class="mb-1 text-sm text-paper">تصمیم این برنامه حفظ شد</p>
            <ul class="max-h-40 divide-y divide-white/5 overflow-auto text-sm">
              <li v-for="k in report.kept" :key="k.name_seen" class="py-1.5">
                <span class="text-paper">{{ k.name_seen }}</span> — این برنامه: {{ k.mine_product ?? howLabel[k.mine_how] }} ({{ howLabel[k.mine_how] ?? k.mine_how }})، فایل: {{ k.file_product ?? howLabel[k.file_how] }} ({{ howLabel[k.file_how] ?? k.file_how }})
              </li>
            </ul>
          </div>
          <div v-if="report.missing.length">
            <p class="mb-1 text-sm text-paper">ثبت نشد؛ محصول در فهرست فعلی لاندا نیست و قطعه دوباره در بازبینی پرسیده می‌شود</p>
            <ul class="max-h-40 divide-y divide-white/5 overflow-auto text-sm">
              <li v-for="m in report.missing" :key="m.name_seen" class="py-1.5"><span class="text-paper">{{ m.name_seen }}</span> → {{ m.product_name ?? m.product_id }}</li>
            </ul>
          </div>
          <div class="flex justify-end border-t hairline pt-4"><button type="button" class="btn-primary" @click="closeImport">بستن</button></div>
        </div>
      </template>
    </UModal>
  </div>
</template>
