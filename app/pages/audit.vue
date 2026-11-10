<script setup lang="ts">
// The audit (plan 130 R-i): every shelf row weighed against the evidence before anything is written.
// It reads only — Landau's registry and shelf, Barezpakhsh's price today, the links judged on a copy
// of the database — and writes nothing. A listed row is corrected by the seller: its links fixed on
// the Matches page, then a price refresh, whose report shows the cost before and after.
import { save } from '@tauri-apps/plugin-dialog';
import { documentDir, homeDir, join } from '@tauri-apps/api/path';
import type { Audit, BridgeError } from '~/types/bridge';

const toast = useToast();
const audit = ref<Audit | null>(null);
const error = ref<BridgeError | null>(null);
const busy = ref<'run' | 'export' | null>(null);

const at = (iso: string) => new Intl.DateTimeFormat('fa-IR', { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(iso));

// PI-29: Matches' pill words for a rename («نام تازه در بارزپخش», «نام تازهٔ تأییدشده»); the sentence
// beside the pill says whether Matches asks about this name, an older one, or not yet
const renameLabel = { WAITING: 'نام تازه در بارزپخش', ASKED_OLDER: 'نام تازه در بارزپخش', UNASKED: 'نام تازه در بارزپخش', CONFIRMED: 'نام تازهٔ تأییدشده' } as const;

// Every row an automatic link last wrote (PI-21, Q-6): the gate passed it, yet a wrong part can
// pass when Landau lacks the true one, and only the seller can see that. Newest write first, paged.
const automatic = computed(() => audit.value?.automatic ?? []);
const writtenAutomatically = computed(() => new Set(automatic.value.map((r) => r.stock_id)));
const { page: autoPage, rows: autoRows } = usePaged(automatic, PAGE_SIZE.table);

const run = async () => {
  error.value = null; busy.value = 'run';
  try { audit.value = await bridge.auditRun(); } catch (e) { error.value = e as BridgeError; } finally { busy.value = null; }
};

// The file is written by the Rust side at the path the native dialog answers (Documents first: given
// a bare name, GTK opens the AppImage's read-only mount — see matches.vue).
const inTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
const startDir = async () => { try { return await documentDir(); } catch { return await homeDir(); } };
const asPath = (p: unknown) => (typeof p === 'string' ? p : (p as { path: string } | null)?.path ?? null);
const exportCsv = async () => {
  error.value = null;
  try {
    const path = inTauri()
      ? asPath(await save({ defaultPath: await join(await startDir(), `landau-bridge-audit-${new Date().toISOString().slice(0, 10)}.csv`), filters: [{ name: 'CSV', extensions: ['csv'] }] }))
      : 'dev://audit.csv';
    if (!path) return;
    busy.value = 'export';
    const n = await bridge.auditExport(path);
    toast.add({ title: `${faNumber(n)} ردیف در فایل ذخیره شد`, color: 'success' });
  } catch (e) { error.value = e as BridgeError; } finally { busy.value = null; }
};
</script>

<template>
  <div>
    <PageHeader title="بازبینی قفسه" lede="هر ردیف قفسه با شواهد سنجیده می‌شود: کدام پیوند اثبات می‌شود و بهای خرید باید چند باشد. چیزی نوشته نمی‌شود.">
      <template #actions>
        <UButton color="neutral" variant="outline" icon="i-lucide-file-spreadsheet" :disabled="!audit || !!busy" :loading="busy === 'export'" @click="exportCsv">ذخیرهٔ CSV</UButton>
        <UButton icon="i-lucide-scan-search" :disabled="!!busy" :loading="busy === 'run'" @click="run">{{ audit ? 'بازبینی دوباره' : 'بازبینی' }}</UButton>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <GlassCard v-if="busy === 'run'" data-reveal>
      <p class="text-sm text-paper-dim">فهرست کامل لاندا، قفسه و قیمت روز بارزپخش خوانده می‌شود؛ چند دقیقه طول می‌کشد.</p>
    </GlassCard>

    <template v-else-if="audit">
      <p class="mb-1 text-sm text-paper" data-reveal>{{ auditSummary(audit) }}</p>
      <p class="mb-6 text-xs text-paper-mute" data-reveal>
        قفسه و قیمت‌ها خوانده‌شده {{ at(audit.read_at) }} · فهرست لاندا {{ at(audit.registry_read_at) }}
      </p>

      <GlassCard v-if="!audit.rows.length" :pad="false" data-reveal>
        <EmptyState art="shelf" title="تردیدی در بهای ردیف‌ها نیست" :description="automatic.length ? 'بهای خرید هر ردیف پیونددار را پیوندهای اثبات‌شده‌اش تأیید می‌کنند. نوشته‌های خودکار پایین‌تر را خودتان هم ببینید.' : 'بهای خرید هر ردیف پیونددار را پیوندهای اثبات‌شده‌اش تأیید می‌کنند.'" />
      </GlassCard>

      <div v-else class="flex flex-col gap-4">
        <GlassCard v-for="row in audit.rows" :key="row.stock_id" data-reveal>
          <div class="flex flex-wrap items-start justify-between gap-x-6 gap-y-2">
            <div class="min-w-0">
              <p class="font-semibold text-paper">{{ row.product_name ?? row.product_id }}</p>
              <p class="text-xs text-paper-mute">
                <span v-if="row.brand">{{ row.brand }} · </span>ردیف <bdi dir="ltr" class="font-mono">{{ row.stock_id }}</bdi>
              </p>
            </div>
            <div class="flex flex-wrap gap-1.5">
              <StatusPill v-for="d in row.doubts" :key="d" tone="warn" :label="doubtText(d)" />
              <StatusPill v-if="writtenAutomatically.has(row.stock_id)" tone="muted" label="نوشتهٔ خودکار هم هست" />
            </div>
          </div>

          <dl class="mt-4 grid grid-cols-1 gap-x-8 gap-y-2 text-sm sm:grid-cols-3">
            <div>
              <dt class="data-label">روی قفسه (لاندا)</dt>
              <dd class="tabular text-paper">{{ money(row.cost) }}</dd>
              <dd class="text-xs text-paper-mute">موجودی {{ faNumber(row.inventory) }}</dd>
            </div>
            <div>
              <dt class="data-label">بر پایهٔ شواهد</dt>
              <dd class="tabular text-paper">{{ row.should_cost == null ? 'نامعلوم' : money(row.should_cost) }}</dd>
              <dd class="text-xs text-paper-mute" title="قیمت روز بارزپخش × (۱ + ارزش افزوده) از پیوندهای اثبات‌شده، محاسبهٔ پل">از پیوندهای اثبات‌شده</dd>
              <dd v-if="row.should_unavailable" class="text-xs text-warn">در بارزپخش ناموجود؛ به‌روزرسانی بهای بالاتر ردیف را پایین نمی‌آورد</dd>
            </div>
            <div>
              <dt class="data-label">اختلاف</dt>
              <dd :class="row.difference ? 'text-warn' : 'text-paper'">{{ differenceText(row) }}</dd>
            </div>
          </dl>

          <ul class="mt-4 flex flex-col gap-2 border-t border-line pt-3 text-sm">
            <li v-for="l in row.links" :key="l.name" class="flex flex-col gap-0.5">
              <p class="flex flex-wrap items-center gap-2">
                <StatusPill :tone="l.proven ? 'ok' : 'danger'" :label="l.proven ? 'اثبات‌شده' : 'اثبات‌نشده'" />
                <StatusPill v-if="l.rename" :tone="l.rename.state === 'CONFIRMED' ? 'muted' : 'warn'" :label="renameLabel[l.rename.state]" />
                <span class="text-paper">{{ l.name }}</span>
                <bdi dir="ltr" class="font-mono text-xs text-paper-mute">{{ l.code }}</bdi>
              </p>
              <p v-if="renameText(l)" class="text-xs" :class="l.rename && l.rename.state !== 'CONFIRMED' ? 'text-warn' : 'text-paper-mute'">
                {{ renameText(l) }}
                <NuxtLink v-if="l.rename?.state === 'WAITING'" to="/matches" class="text-crimson-300 hover:underline">باز کردن تطبیق‌ها</NuxtLink>
              </p>
              <p v-if="l.reasons.length" class="text-xs text-warn">{{ l.reasons.map(reasonText).join('؛ ') }}</p>
              <p v-if="linkToday(l)" class="text-xs text-paper-mute">{{ linkToday(l) }}</p>
              <p v-if="linkUnits(l)" class="text-xs text-paper-mute">{{ linkUnits(l) }}</p>
            </li>
          </ul>

          <p v-if="row.last_write" class="mt-3 text-xs text-paper-mute">
            آخرین بهای نوشته‌شده {{ faNumber(row.last_write.cost) }} ریال، در عملیات
            <NuxtLink :to="`/jobs/${row.last_write.job_id}`" class="hover:text-crimson-300">#{{ faNumber(row.last_write.job_id) }}</NuxtLink>،
            از {{ row.last_write.names.map((n) => `«${n}»`).join(' و ') }}
          </p>
          <p v-if="row.unproven_units" class="mt-1 text-xs text-warn">
            {{ faNumber(row.unproven_units) }} واحد از موجودی این ردیف از کالاهایی منتقل شده که پیوندشان اینجا اثبات نمی‌شود.
          </p>
        </GlassCard>

        <GlassCard data-reveal>
          <p class="text-sm text-paper-dim">
            اصلاح با شماست: پیوندهای ردیف را در
            <NuxtLink to="/matches" class="text-crimson-300 hover:underline">تطبیق‌ها</NuxtLink>
            درست کنید و سپس قیمت‌ها را به‌روز کنید؛ گزارش آن عملیات بهای خرید پیش و پس از نوشتن را نشان می‌دهد.
          </p>
        </GlassCard>
      </div>
    </template>

    <GlassCard v-else :pad="false" data-reveal>
      <EmptyState art="shelf" title="هنوز بازبینی‌ای انجام نشده" description="«بازبینی» قفسهٔ فروشگاه را با پیوندها و قیمت روز می‌سنجد و فقط می‌خواند." />
    </GlassCard>

    <section v-if="audit && busy !== 'run' && automatic.length" class="mt-8" data-reveal>
      <div class="mb-3 flex flex-wrap items-baseline justify-between gap-2">
        <h2 class="display text-xl text-paper">نوشته‌های خودکار <span class="tabular text-paper-mute">({{ faNumber(automatic.length) }})</span></h2>
        <span class="text-xs text-paper-mute">تازه‌ترین نوشتن بالاتر</span>
      </div>
      <p class="mb-4 text-sm text-paper-dim">
        آخرین بهای این ردیف‌ها را پیوندی نوشته که پل خودش، بی‌پرسش از شما، برگزیده است. وضع امروز هر پیوند کنار آن آمده؛
        اما حتی پیوند اثبات‌شده هم، اگر قطعهٔ درست در فهرست لاندا نباشد، ممکن است قطعه‌ای شبیه آن باشد. نام کالای بارزپخش
        را با قطعهٔ لاندا مقایسه کنید. اگر یکی نیستند، پیوندش را در
        <NuxtLink to="/matches" class="text-crimson-300 hover:underline">تطبیق‌ها</NuxtLink>
        درست کنید و قیمت‌ها را به‌روز کنید؛ اگر درست است، همان قطعه را آنجا خودتان برگزینید تا تصمیم شما شود و از این
        فهرست بیرون برود.
      </p>
      <LedgerTable class="glass">
        <thead><tr><th>قطعهٔ لاندا</th><th>کالای بارزپخش</th><th class="text-end">بهای خرید روی قفسه</th><th>آخرین نوشتن</th></tr></thead>
        <tbody>
          <tr v-for="r in autoRows" :key="r.stock_id">
            <td class="min-w-48">
              <p class="text-paper">{{ r.product_name ?? r.product_id }}</p>
              <p class="text-xs text-paper-mute"><span v-if="r.brand">{{ r.brand }} · </span>موجودی {{ faNumber(r.inventory) }}</p>
              <StatusPill v-if="r.doubted" tone="warn" label="در فهرست تردیدها هم هست" class="mt-1" />
            </td>
            <td class="min-w-48">
              <div v-for="l in r.links" :key="l.name" class="space-y-0.5">
                <p class="flex flex-wrap items-center gap-2 text-paper-dim">
                  <StatusPill :tone="l.proven ? 'ok' : 'danger'" :label="l.proven ? 'اثبات‌شده' : 'اثبات‌نشده'" />
                  <StatusPill v-if="l.rename" :tone="l.rename.state === 'CONFIRMED' ? 'muted' : 'warn'" :label="renameLabel[l.rename.state]" />
                  <span>{{ l.name }}</span> <bdi dir="ltr" class="font-mono text-xs text-paper-mute">{{ l.code }}</bdi>
                </p>
                <p v-if="renameText(l)" class="text-xs" :class="l.rename && l.rename.state !== 'CONFIRMED' ? 'text-warn' : 'text-paper-mute'">{{ renameText(l) }}</p>
                <p v-if="l.reasons.length" class="text-xs text-warn">{{ l.reasons.map(reasonText).join('؛ ') }}</p>
              </div>
            </td>
            <td class="whitespace-nowrap text-end tabular">
              {{ r.cost == null ? '—' : money(r.cost) }}
              <div v-if="writtenCost(r)" class="text-xs text-warn">{{ writtenCost(r) }}</div>
            </td>
            <td class="whitespace-nowrap text-xs text-paper-mute">
              <NuxtLink :to="`/jobs/${r.last_write.job_id}`" class="hover:text-crimson-300">عملیات #{{ faNumber(r.last_write.job_id) }}</NuxtLink>
              <div class="tabular">{{ isoToLocal(r.last_write.at) }}</div>
            </td>
          </tr>
        </tbody>
        <template #footer><Pager v-model:page="autoPage" :total="automatic.length" :items-per-page="PAGE_SIZE.table" label="ردیف" /></template>
      </LedgerTable>
    </section>

    <p class="mt-6 text-xs text-paper-mute">
      فروشی که در صندوق درست در فاصلهٔ خواندن و نوشتن موجودی انجام شود ممکن است موجودی را بیش از واقع نشان دهد؛ این پنجرهٔ کوتاه تا آمدن رسید لاندا باقی است.
    </p>
  </div>
</template>
