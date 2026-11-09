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
        <EmptyState art="shelf" title="ردیفی نیازمند بررسی نیست" description="بهای خرید هر ردیف پیونددار را پیوندهای اثبات‌شده‌اش تأیید می‌کنند." />
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
                <span class="text-paper">{{ l.name }}</span>
                <bdi dir="ltr" class="font-mono text-xs text-paper-mute">{{ l.code }}</bdi>
              </p>
              <p v-if="l.reasons.length" class="text-xs text-warn">{{ l.reasons.map(reasonText).join('؛ ') }}</p>
              <p v-if="linkToday(l)" class="text-xs text-paper-mute">{{ linkToday(l) }}</p>
              <p v-if="l.units" class="text-xs text-paper-mute">{{ faNumber(l.units) }} واحد از این کالا به این ردیف منتقل شده</p>
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

    <p class="mt-6 text-xs text-paper-mute">
      فروشی که در صندوق درست در فاصلهٔ خواندن و نوشتن موجودی انجام شود ممکن است موجودی را بیش از واقع نشان دهد؛ این پنجرهٔ کوتاه تا آمدن رسید لاندا باقی است.
    </p>
  </div>
</template>
