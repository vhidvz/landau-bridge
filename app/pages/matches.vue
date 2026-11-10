<script setup lang="ts">
import { documentDir, homeDir, join } from '@tauri-apps/api/path';
import { open, save } from '@tauri-apps/plugin-dialog';
import type { BridgeError, CandidateView, DecisionImportReport, Inspection, MatchRow, Scored, UndecidedRow } from '~/types/bridge';

const rows = ref<MatchRow[]>([]);
const undecided = ref<UndecidedRow[]>([]);
const error = ref<BridgeError | null>(null);
const route = useRoute();
const filter = ref<'ALL' | 'AUTO' | 'USER' | 'NONE' | 'REVIEW' | 'UNDECIDED'>(route.query.filter === 'REVIEW' ? 'REVIEW' : 'ALL');
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

const inFilter = (r: MatchRow) => filter.value === 'ALL' || (filter.value === 'REVIEW' ? linkAsksSeller(r) : r.how === filter.value);
const shown = computed(() => rows.value.filter((r) => inFilter(r) && matchesQuery(query.value, [r.name_seen, r.code, r.product_name])));
const toReview = computed(() => rows.value.filter(linkAsksSeller).length);
const shownUndecided = computed(() => undecided.value.filter((u) => matchesQuery(query.value, [u.name_seen, u.code, ...u.candidates.map((c) => c.candidate.name)])));
const PAGE = PAGE_SIZE.table;
const { page: matchPage, rows: matchRows } = usePaged(shown, PAGE, [filter, query]);
const { page: undecidedPage, rows: undecidedRows, showIndex: showUndecided } = usePaged(shownUndecided, PAGE, [filter, query]);
const howLabel: Record<string, string> = { AUTO: 'خودکار', USER: 'انتخاب شما', NONE: 'در لاندا نیست' };
const filters = computed(() => [
  { value: 'ALL', label: 'همه' }, { value: 'AUTO', label: 'خودکار' }, { value: 'USER', label: 'انتخاب شما' }, { value: 'NONE', label: 'در لاندا نیست' },
  { value: 'REVIEW', label: toReview.value ? `نیازمند بررسی · ${faNumber(toReview.value)}` : 'نیازمند بررسی' },
  { value: 'UNDECIDED', label: undecided.value.length ? `بدون تصمیم · ${faNumber(undecided.value.length)}` : 'بدون تصمیم' },
] as const);

// ---- what each pick would be (plan 130 R-k), read for the dialog's candidates
const inspection = ref<Inspection | null>(null);
const inspecting = ref(false);
const inspectFailed = ref<string | null>(null);
let inspectSeq = 0;
const inspect = async () => {
  const target = deciding.value ?? editing.value;
  const ids = candidates.value.map((c) => c.candidate.id);
  inspection.value = null;
  inspectFailed.value = null;
  const seq = ++inspectSeq;
  if (!target || !ids.length) return;
  inspecting.value = true;
  try {
    const got = await bridge.reviewInspect(target.name_seen, target.code, ids);
    if (seq === inspectSeq) inspection.value = got;
  } catch (e) { if (seq === inspectSeq) inspectFailed.value = (e as BridgeError).message; } finally { if (seq === inspectSeq) inspecting.value = false; }
};
const viewOf = (id: string): CandidateView | undefined => inspection.value?.candidates.find((c) => c.product.id === id);
const opened = reactive<Record<string, boolean>>({});
const { pending, busy: picking, pick: confirmPick, respond } = usePick();

const openEdit = (r: MatchRow) => { editing.value = r; term.value = r.name_seen; candidates.value = []; void inspect(); void inspectRename(r); };

// ---- PI-28: the site lists the link's code under a name that conflicts with its product. The seller
// says it is the same part; the core judges the pair and asks for the second confirm, as for a pick.
const editRename = computed(() => (editing.value ? waitingRename(editing.value) : null));
const editConfirmedRename = computed(() => (editing.value ? confirmedRename(editing.value) : null));
// the money is read for the name the seller is asked about; the site listing that code under yet
// another name today means this word would write nothing, so it is not offered
const renameMovedOn = computed(() => known(renameInspection.value?.quote)?.renamed === true);
const renameInspection = ref<Inspection | null>(null);
const renameLoading = ref(false);
const renameFailed = ref<string | null>(null);
let renameSeq = 0;
const inspectRename = async (r: MatchRow) => {
  renameInspection.value = null;
  renameFailed.value = null;
  const seq = ++renameSeq;
  const rename = waitingRename(r);
  if (!rename || !r.product_id) return;
  renameLoading.value = true;
  try {
    const got = await bridge.reviewInspect(rename.name, rename.code, [r.product_id]);
    if (seq === renameSeq) renameInspection.value = got;
  } catch (e) { if (seq === renameSeq) renameFailed.value = (e as BridgeError).message; } finally { if (seq === renameSeq) renameLoading.value = false; }
};
const confirmRename = async () => {
  // the link and its rename as the seller sees them, captured at the click
  const r = editing.value;
  const rename = editRename.value;
  if (!r?.product_id || !rename) return;
  const product = r.product_id;
  error.value = null;
  let recorded = true;
  try {
    const done = await confirmPick(
      async (confirmed) => { recorded = await bridge.linkRenameConfirm(r.name_seen, r.link_version, product, rename.code, rename.name, confirmed); },
      {
        supplierName: rename.name,
        product: { name: r.product_name ?? '', brand: r.product_brand },
        candidate: renameInspection.value?.candidates.find((c) => c.product.id === product) ?? null,
        inspection: renameInspection.value,
      },
    );
    if (!done) return;
    editing.value = null;
    toast.add(recorded
      ? { title: 'نام تازه تأیید شد؛ از اجرای بعدی قیمت این کد نوشته می‌شود', color: 'success' }
      : { title: 'این نام با قطعه جور است و تأیید نمی‌خواهد؛ اجرای بعدی دوباره می‌سنجد', color: 'info' });
    await load();
  } catch (e) { error.value = e as BridgeError; }
};
const withdrawRename = async () => {
  const r = editing.value;
  if (!r) return;
  error.value = null;
  try {
    await bridge.linkRenameWithdraw(r.name_seen, r.link_version);
    editing.value = null;
    toast.add({ title: 'تأیید نام تازه پس گرفته شد؛ اجرای بعدی دوباره می‌پرسد', color: 'success' });
    await load();
  } catch (e) { error.value = e as BridgeError; }
};
// An undecided part opens on the candidates review showed — no new search; one is there if wanted.
const openDecide = (u: UndecidedRow | null) => { deciding.value = u; term.value = u?.name_seen ?? ''; candidates.value = u?.candidates ?? []; void inspect(); };
const search = async () => {
  searching.value = true;
  try { candidates.value = await bridge.reviewSearch(term.value); void inspect(); } catch (e) { error.value = e as BridgeError; } finally { searching.value = false; }
};
const decided = async (u: UndecidedRow) => {
  const next = nextAfterDecision(shownUndecided.value, u.name_key);
  undecided.value = undecided.value.filter((x) => x.name_key !== u.name_key);
  openDecide(next);
  rows.value = await bridge.matchesList();
};
const pick = async (c: Scored) => {
  // the part being decided, captured: the seller may move on («بعداً») while the pick is in flight
  const wasDeciding = deciding.value;
  const target = wasDeciding ?? editing.value;
  if (!target) return;
  error.value = null;
  try {
    const done = await confirmPick(
      (confirmed) => bridge.matchSet(target.name_seen, target.code, c.candidate.id, confirmed),
      { supplierName: target.name_seen, product: { name: c.candidate.name, brand: c.candidate.brand }, candidate: viewOf(c.candidate.id) ?? null, inspection: inspection.value },
    );
    if (!done) return;
    if (wasDeciding) await decided(wasDeciding); else { editing.value = null; await load(); }
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
    <PageHeader title="تطبیق‌ها" lede="هر کالای بارزپخش به کدام قطعهٔ لاندا وصل است.">
      <template #actions>
        <UButton color="neutral" variant="outline" icon="i-lucide-download" :disabled="!!busy" :loading="busy === 'export'" @click="exportDecisions">ذخیرهٔ تصمیم‌ها</UButton>
        <UButton color="neutral" variant="outline" icon="i-lucide-upload" :disabled="!!busy" @click="chooseImport">بازیابی تصمیم‌ها</UButton>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <div class="mb-4 flex flex-wrap items-center gap-3" data-reveal>
      <SegmentedControl v-model="filter" :items="filters" aria-label="نوع تطبیق" />
      <div class="relative ms-auto w-72">
        <input v-model="query" type="search" class="field pe-9" placeholder="جست‌وجو در نام یا کد" aria-label="جست‌وجو در تطبیق‌ها" />
        <UIcon name="i-lucide-search" class="pointer-events-none absolute inset-y-0 end-3 my-auto size-4 text-paper-mute" aria-hidden="true" />
      </div>
    </div>

    <template v-if="filter === 'UNDECIDED'">
      <LedgerTable v-if="shownUndecided.length" class="glass" data-reveal>
        <thead>
          <tr><th>نام در فاکتور</th><th>نزدیک‌ترین پیشنهاد</th><th>شباهت</th><th>دفعات</th><th>آخرین بار</th><th></th></tr>
        </thead>
        <tbody>
          <tr v-for="u in undecidedRows" :key="u.name_key">
            <td class="w-1/2 max-w-0">
              <p class="truncate font-medium text-paper" :title="u.name_seen">{{ u.name_seen }}</p>
              <p class="truncate text-xs text-paper-mute"><bdi dir="ltr">{{ u.code }}</bdi></p>
            </td>
            <td class="w-1/2 max-w-0"><p class="truncate" :title="u.candidates[0]?.candidate.name">{{ u.candidates[0]?.candidate.name ?? '—' }}</p></td>
            <td><SimilarityBar v-if="u.candidates[0]" :value="u.candidates[0].similarity" /><span v-else class="text-paper-mute">—</span></td>
            <td class="tabular">{{ faNumber(u.times) }}</td>
            <td class="whitespace-nowrap">{{ isoToDate(u.seen_at) }}</td>
            <td class="text-end"><button type="button" class="btn-primary btn-sm" @click="openDecide(u)">تصمیم</button></td>
          </tr>
        </tbody>
        <template #footer><Pager v-model:page="undecidedPage" :total="shownUndecided.length" :items-per-page="PAGE" label="قطعهٔ بدون تصمیم" /></template>
      </LedgerTable>
      <GlassCard v-if="!shownUndecided.length" :pad="false" data-reveal>
        <EmptyState
          :title="undecided.length ? 'قطعه‌ای با این عبارت نیست' : 'قطعهٔ بدون تصمیمی نمانده'"
          :description="undecided.length ? 'عبارت جست‌وجو را عوض کنید.' : 'قطعه‌هایی که در بازبینی «بعداً» زده‌اید اینجا می‌مانند.'"
        />
      </GlassCard>
    </template>
    <template v-else>
      <LedgerTable v-if="shown.length" class="glass" data-reveal>
        <thead>
          <tr><th>نام در فاکتور</th><th>قطعهٔ لاندا</th><th>نحوه</th><th>شباهت</th><th>تاریخ</th><th></th></tr>
        </thead>
        <tbody>
          <tr v-for="r in matchRows" :key="r.name_key">
            <td class="w-1/2 max-w-0">
              <p class="truncate font-medium text-paper" :title="r.name_seen">{{ r.name_seen }}</p>
              <p class="truncate text-xs text-paper-mute"><bdi dir="ltr">{{ r.code }}</bdi></p>
            </td>
            <td class="w-1/2 max-w-0"><p class="truncate" :class="!r.product_name && 'text-paper-mute'" :title="r.product_name ?? undefined">{{ r.product_name ?? '—' }}</p></td>
            <td class="whitespace-nowrap"><span class="inline-flex gap-1.5"><StatusPill :tone="r.how === 'NONE' ? 'danger' : r.how === 'USER' ? 'ok' : 'info'" :label="howLabel[r.how] ?? r.how" /><StatusPill v-if="r.how !== 'NONE'" :tone="linkStateTone[r.state]" :label="linkStateLabel[r.state]" /><StatusPill v-if="waitingRename(r)" tone="warn" label="نام تازه در بارزپخش" /><StatusPill v-else-if="confirmedRename(r)" tone="muted" label="نام تازهٔ تأییدشده" /></span></td>
            <td><SimilarityBar v-if="r.similarity != null" :value="r.similarity" /><span v-else class="text-paper-mute">—</span></td>
            <td class="whitespace-nowrap">{{ isoToDate(r.decided_at) }}</td>
            <td class="text-end"><button type="button" class="btn-ghost btn-sm" @click="openEdit(r)">تغییر</button></td>
          </tr>
        </tbody>
        <template #footer><Pager v-model:page="matchPage" :total="shown.length" :items-per-page="PAGE" label="تطبیق" /></template>
      </LedgerTable>
      <GlassCard v-if="!shown.length" :pad="false" data-reveal>
        <EmptyState
          :title="rows.length ? 'تطبیقی با این فیلتر نیست' : 'هنوز تطبیقی ثبت نشده'"
          :description="rows.length ? 'فیلتر یا عبارت جست‌وجو را عوض کنید.' : 'با اولین وارد کردن فاکتورها پر می‌شود.'"
        >
          <template v-if="!rows.length" #action><NuxtLink to="/import" class="btn-ghost"><UIcon name="i-lucide-file-input" class="size-4" />وارد کردن فاکتورها</NuxtLink></template>
        </EmptyState>
      </GlassCard>
    </template>

    <UModal :open="!!editing || !!deciding" :title="deciding ? 'تصمیم برای قطعه' : 'تغییر تطبیق'" :ui="{ content: 'sm:max-w-3xl' }" @update:open="(v: boolean) => { if (!v) { editing = null; deciding = null; } }">
      <template #body>
        <div v-if="editing || deciding" class="space-y-4">
          <p><span class="font-medium text-paper">{{ (deciding ?? editing)!.name_seen }}</span> <bdi dir="ltr" class="whitespace-nowrap text-sm text-paper-mute">{{ (deciding ?? editing)!.code }}</bdi></p>
          <p v-if="deciding" class="text-xs text-paper-mute">
            {{ faNumber(position) }} از {{ faNumber(shownUndecided.length) }} · {{ faNumber(deciding.times) }} بار بدون تصمیم ماند، آخرین بار {{ isoToLocal(deciding.seen_at) }}. پیشنهادهای همان بازبینی:
          </p>
          <ul v-if="editing && needsReview(editing.state) && storedReasons(editing.state_reason).length" class="space-y-1 rounded-lg border border-warn/30 bg-warn/6 px-4 py-2.5 text-xs text-paper" aria-label="چرا این پیوند بررسی می‌خواهد">
            <li v-for="(r, i) in storedReasons(editing.state_reason)" :key="i">{{ reasonText(r) }}</li>
          </ul>
          <section v-if="editing && editRename" class="space-y-3 rounded-lg border border-warn/30 bg-warn/6 px-4 py-3 text-sm text-paper" aria-label="نام تازه در بارزپخش">
            <p>بارزپخش کد <bdi dir="ltr">{{ editRename.code }}</bdi> را اکنون با این نام می‌فروشد: <span class="font-medium">{{ editRename.name }}</span></p>
            <p class="text-xs text-paper-mute">این نام با «{{ editing.product_name }}» جور نیست، پس قیمتش روی قفسه نوشته نمی‌شود تا شما بگویید همان قطعه است.</p>
            <ul class="space-y-1 text-xs" aria-label="ناسازگاری‌های نام تازه">
              <li v-for="(x, i) in editRename.reasons" :key="i" class="flex gap-2"><UIcon name="i-lucide-triangle-alert" class="mt-0.5 size-3.5 shrink-0 text-warn" aria-hidden="true" />{{ reasonText(x) }}</li>
            </ul>
            <MoneyLine :inspection="renameInspection" :loading="renameLoading" :failed="renameFailed" />
            <p v-if="renameMovedOn" class="text-xs text-warn">بارزپخش امروز این کد را با نام دیگری می‌فروشد؛ اجرای بعدی نام تازه را دوباره می‌سنجد و اگر لازم باشد دوباره می‌پرسد.</p>
            <div class="flex justify-end"><button type="button" class="btn-ghost btn-sm" :disabled="picking || renameMovedOn" @click="confirmRename">همان قطعه است</button></div>
          </section>
          <section v-else-if="editing && editConfirmedRename" class="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-white/10 px-4 py-3 text-sm text-paper" aria-label="نام تازهٔ تأییدشده">
            <p>شما تأیید کرده‌اید که کد <bdi dir="ltr">{{ editConfirmedRename.code }}</bdi> با نام «{{ editConfirmedRename.name }}» همین قطعه است؛ قیمت آن با همین نام نوشته می‌شود.</p>
            <button type="button" class="btn-ghost btn-sm" @click="withdrawRename">پس گرفتن تأیید</button>
          </section>
          <MoneyLine v-if="candidates.length" :inspection="inspection" :loading="inspecting" :failed="inspectFailed" />
          <div class="flex gap-2">
            <input v-model="term" class="field flex-1" aria-label="عبارت جست‌وجو" @keyup.enter="search" />
            <UButton :loading="searching" icon="i-lucide-search" @click="search">جست‌وجو در لاندا</UButton>
          </div>
          <p v-if="deciding && !candidates.length" class="text-sm text-paper-mute">لاندا در آن بازبینی پیشنهادی نداشت. عبارت دیگری را جست‌وجو کنید یا «در لاندا نیست» را بزنید.</p>
          <ul class="max-h-72 divide-y divide-white/5 overflow-auto">
            <li v-for="c in candidates" :key="c.candidate.id" class="py-2.5">
              <div class="flex items-center justify-between gap-3">
                <span class="min-w-0 truncate text-paper">{{ c.candidate.name }} <span v-if="c.candidate.brand" class="text-xs text-paper-mute">{{ c.candidate.brand }}</span></span>
                <span class="flex shrink-0 items-center gap-3">
                  <ShelfCost :view="viewOf(c.candidate.id)" :failed="inspectFailed" />
                  <button v-if="viewOf(c.candidate.id)" type="button" class="rounded-full" :aria-expanded="!!opened[c.candidate.id]" title="شواهد" @click="opened[c.candidate.id] = !opened[c.candidate.id]"><StatusPill :tone="pickTone(viewOf(c.candidate.id)!.verdict)" :label="pickLabel(viewOf(c.candidate.id)!.verdict)" /></button>
                  <SimilarityBar :value="c.similarity" /><button type="button" class="btn-primary btn-sm" :disabled="picking" @click="pick(c)">انتخاب</button>
                </span>
              </div>
              <div v-if="opened[c.candidate.id] && viewOf(c.candidate.id)" class="glass-inset mt-2 px-4 py-3"><EvidencePanel :evidence="viewOf(c.candidate.id)!.evidence" /></div>
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

    <PickConfirm :pending="pending" @answer="respond" />

    <UModal :open="!!importing" title="بازیابی تصمیم‌ها" @update:open="(v: boolean) => { if (!v && busy !== 'import') closeImport(); }">
      <template #body>
        <div v-if="!report" class="space-y-4">
          <p class="text-sm">تصمیم‌های فایل با تصمیم‌های این برنامه ادغام می‌شوند. تصمیمی که شما گرفته‌اید (انتخاب یا «در لاندا نیست») بر تطبیق خودکار مقدم است. محصول هر تصمیم پیش از ثبت در فهرست فعلی لاندا بررسی می‌شود و اگر دیگر نباشد ثبت نمی‌شود.</p>
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
