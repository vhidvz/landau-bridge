<script setup lang="ts">
import { openUrl } from '@tauri-apps/plugin-opener';
import type { BridgeError, LandauStatus, SettingsView, UpdateView } from '~/types/bridge';

const DEFAULT_URL = 'https://api.landau.ir';
const toast = useToast();
const view = ref<SettingsView | null>(null);
const error = ref<BridgeError | null>(null);
const landau = ref<LandauStatus | null>(null);
const busy = ref<'landau' | 'supplier' | 'save' | null>(null);
const savedTick = useSettingsSaved();
const advancedOpen = ref(false);

const supplierOk = ref(false);
const landauPill = computed(() =>
  landau.value ? { label: `متصل به ${landau.value.business_name ?? landau.value.business}`, tone: 'ok' as const }
    : view.value?.has_landau_token ? { label: 'کلید ذخیره‌شده', tone: 'muted' as const }
      : { label: 'آزمایش‌نشده', tone: 'warn' as const });
const supplierPill = computed(() =>
  supplierOk.value ? { label: 'ورود موفق', tone: 'ok' as const }
    : view.value?.has_supplier_password ? { label: 'رمز ذخیره‌شده', tone: 'muted' as const }
      : { label: 'آزمایش‌نشده', tone: 'warn' as const });

// overwrite_manual and fetch_since_only are edited where they apply (refresh / import pages), not here.
const form = reactive({
  landau_url: '', landau_token: '', store: '', supplier_user: '', supplier_password: '',
  default_authenticity: 'ORIGINAL' as 'ORIGINAL' | 'FAKE', threshold: 80, vat_percent: 10, min_margin_percent: 15,
});

const load = async () => {
  try {
    view.value = await bridge.settingsGet();
    Object.assign(form, {
      landau_url: view.value.landau_url, store: view.value.store ?? '', supplier_user: view.value.supplier_user ?? '',
      default_authenticity: view.value.default_authenticity, threshold: view.value.threshold, vat_percent: view.value.vat_percent,
      min_margin_percent: view.value.min_margin_percent, landau_token: '', supplier_password: '',
    });
    advancedOpen.value = view.value.landau_url !== DEFAULT_URL;
  } catch (e) { error.value = e as BridgeError; }
};
onMounted(load);

// The app's own version: shown, checked and installed here; state changes arrive on update://state.
const RELEASES = 'https://github.com/vhidvz/landau-bridge/releases';
const upd = ref<UpdateView | null>(null);
const updBusy = ref<'check' | 'install' | null>(null);
const updLine = computed(() => (upd.value ? updateLabel(upd.value.state, upd.value.kind) : null));
let offUpdate: (() => void) | null = null;
onMounted(async () => {
  try { upd.value = await bridge.updateStatus(); } catch (e) { error.value = e as BridgeError; }
  offUpdate = await onUpdateState((s) => { if (upd.value) upd.value = { ...upd.value, state: s }; });
});
onBeforeUnmount(() => offUpdate?.());
const checkUpdate = async () => {
  error.value = null; updBusy.value = 'check';
  try { upd.value = await bridge.updateCheck(); } catch (e) { error.value = e as BridgeError; } finally { updBusy.value = null; }
};
// On success the app restarts (Linux) or the installer takes over (Windows): nothing to do after.
const installUpdate = async () => {
  error.value = null; updBusy.value = 'install';
  try { await bridge.updateInstall(); } catch (e) {
    error.value = e as BridgeError;
    upd.value = await bridge.updateStatus().catch(() => upd.value);
  } finally { updBusy.value = null; }
};
const autoUpdate = computed({
  get: () => upd.value?.auto_update ?? true,
  set: async (on: boolean) => {
    if (!upd.value) return;
    upd.value = { ...upd.value, auto_update: on };
    try { await bridge.settingsSave({ auto_update: on }); } catch (e) {
      error.value = e as BridgeError;
      upd.value = { ...upd.value, auto_update: !on };
    }
  },
});
const openReleases = async () => {
  try { await openUrl(RELEASES); } catch { toast.add({ title: RELEASES, description: 'این نشانی را در مرورگر باز کنید', color: 'neutral' }); }
};

const save = async () => {
  error.value = null; busy.value = 'save';
  try {
    const storeName = landau.value?.stores.find((s) => s.id === form.store)?.name ?? view.value?.store_name ?? undefined;
    view.value = await bridge.settingsSave({
      landau_url: form.landau_url, landau_token: form.landau_token || undefined, store: form.store || undefined, store_name: storeName ?? undefined,
      supplier_user: form.supplier_user, supplier_password: form.supplier_password || undefined,
      default_authenticity: form.default_authenticity, threshold: form.threshold, vat_percent: form.vat_percent, min_margin_percent: form.min_margin_percent,
    });
    form.landau_token = ''; form.supplier_password = '';
    savedTick.value++;
    toast.add({ title: 'تنظیمات ذخیره شد', color: 'success' });
  } catch (e) { error.value = e as BridgeError; } finally { busy.value = null; }
};

const validateLandau = async () => {
  error.value = null; busy.value = 'landau';
  try {
    view.value = await bridge.settingsSave({ landau_url: form.landau_url, landau_token: form.landau_token || undefined });
    form.landau_token = '';
    landau.value = await bridge.landauValidate();
    if (!form.store && landau.value.stores.length === 1) form.store = landau.value.stores[0]!.id;
    toast.add({ title: `اتصال به لاندا برقرار است: ${landau.value.business_name ?? landau.value.business}`, color: 'success' });
  } catch (e) { error.value = e as BridgeError; } finally { busy.value = null; }
};

const testSupplier = async () => {
  error.value = null; busy.value = 'supplier';
  try {
    view.value = await bridge.settingsSave({ supplier_user: form.supplier_user, supplier_password: form.supplier_password || undefined });
    form.supplier_password = '';
    await bridge.supplierTestLogin();
    supplierOk.value = true;
    toast.add({ title: 'ورود به بارزپخش موفق بود', color: 'success' });
  } catch (e) { error.value = e as BridgeError; } finally { busy.value = null; }
};

const reducedFx = computed({ get: () => useReducedEffects().value, set: (v: boolean) => setReducedEffects(v) });
const copyPath = async () => {
  if (!view.value) return;
  try { await navigator.clipboard.writeText(view.value.data_dir); toast.add({ title: 'کپی شد', color: 'success' }); }
  catch { toast.add({ title: 'دسترسی به کلیپ‌بورد ممکن نبود', color: 'error' }); }
};

const storeItems = computed(() => (landau.value?.stores ?? []).map((s) => ({ label: s.name ?? s.id, value: s.id })));
const authItems = [{ label: 'اصلی', value: 'ORIGINAL' }, { label: 'غیراصلی (کپی)', value: 'FAKE' }];
</script>

<template>
  <div v-reveal class="mx-auto max-w-3xl">
    <PageHeader kicker="تنظیمات · ۰۷" title="دو کرانه را وصل کنید." lede="کلید لاندا و ورود بارزپخش، و چند قاعدهٔ وارد کردن. هر چیز دیگری پیش‌فرض خوبی دارد.">
      <template #actions>
        <UButton :loading="busy === 'save'" icon="i-lucide-save" @click="save">ذخیرهٔ تنظیمات</UButton>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <div class="space-y-6">
      <GlassCard data-reveal>
        <div class="mb-5 flex items-center justify-between gap-3"><h2 class="display text-xl text-paper">لاندا</h2><StatusPill :tone="landauPill.tone" :label="landauPill.label" /></div>
        <div class="space-y-4">
          <UFormField label="کلید API لاندا" :hint="view?.has_landau_token ? 'کلیدی ذخیره شده است؛ برای تغییر، کلید تازه را وارد کنید' : 'در لاندا: حساب کاربری ← کلیدهای API'">
            <UInput v-model="form.landau_token" type="password" dir="ltr" placeholder="apt-…" class="w-full" />
          </UFormField>
          <UButton :loading="busy === 'landau'" color="neutral" variant="outline" icon="i-lucide-plug-zap" @click="validateLandau">بررسی اتصال و دریافت فروشگاه‌ها</UButton>
          <p v-if="landau && landau.active_rules === 0" class="text-sm text-warn">هیچ قاعدهٔ قیمت‌گذاری فعالی ندارید؛ تا زمانی که در لاندا قاعده‌ای بسازید، قیمت قفسه برابر قیمت خرید خواهد بود.</p>
          <p v-else-if="landau" class="tabular text-sm text-paper-mute">{{ faNumber(landau.active_rules) }} قاعدهٔ قیمت‌گذاری فعال</p>
          <UFormField label="فروشگاه مقصد" hint="ردیف‌ها روی قفسهٔ این فروشگاه نوشته می‌شوند">
            <USelect v-model="form.store" :items="storeItems" :disabled="!storeItems.length" placeholder="ابتدا اتصال را بررسی کنید" class="w-full" />
            <p v-if="!storeItems.length && view?.store_name" class="mt-1 text-sm text-paper-mute">فروشگاه ذخیره‌شده: {{ view.store_name }}</p>
          </UFormField>
        </div>
      </GlassCard>

      <GlassCard data-reveal>
        <div class="mb-5 flex items-center justify-between gap-3"><h2 class="display text-xl text-paper">بارزپخش</h2><StatusPill :tone="supplierPill.tone" :label="supplierPill.label" /></div>
        <div class="space-y-4">
          <UFormField label="نام کاربری"><UInput v-model="form.supplier_user" dir="ltr" class="w-full" /></UFormField>
          <UFormField label="کلمه عبور" :hint="view?.has_supplier_password ? 'ذخیره شده؛ برای تغییر، مقدار تازه را وارد کنید' : ''">
            <UInput v-model="form.supplier_password" type="password" dir="ltr" class="w-full" />
          </UFormField>
          <UButton :loading="busy === 'supplier'" color="neutral" variant="outline" icon="i-lucide-log-in" @click="testSupplier">آزمایش ورود</UButton>
        </div>
      </GlassCard>

      <GlassCard data-reveal>
        <h2 class="display mb-5 text-xl text-paper">قواعد وارد کردن</h2>
        <div class="space-y-4">
          <UFormField label="اصالت پیش‌فرض ردیف‌های تازه" hint="وضعیت همیشه «نو» ثبت می‌شود؛ اصالت را می‌توانید برای هر قلم در مرحلهٔ بازبینی تغییر دهید">
            <USelect v-model="form.default_authenticity" :items="authItems" class="w-full" />
          </UFormField>
          <UFormField label="نرخ ارزش افزوده (٪)" hint="قیمت خرید = قیمت فهرست بارزپخش × (۱ + نرخ)">
            <UInput v-model.number="form.vat_percent" type="number" min="0" max="100" step="0.5" dir="ltr" class="w-32" />
          </UFormField>
          <UFormField label="حداقل سود خرده‌فروش (٪)" hint="ردیف تازه با این سود روی قفسه می‌آید تا قاعدهٔ لاندا قیمت نهایی را بگذارد؛ ردیفی که زیر این سود بماند در گزارش هشدار می‌گیرد">
            <UInput v-model.number="form.min_margin_percent" type="number" min="0" max="1000" step="0.5" dir="ltr" class="w-32" />
          </UFormField>
        </div>
      </GlassCard>

      <GlassCard data-reveal>
        <h2 class="display mb-5 text-xl text-paper">نمایش</h2>
        <div class="flex items-center justify-between gap-6">
          <div>
            <div class="text-sm text-paper">جلوه‌های بصری کمتر</div>
            <p class="text-xs text-paper-mute">پنل‌های مات به جای شیشه، بدون هاله و انیمیشن؛ برای رایانه‌های ضعیف. خودکار تشخیص داده می‌شود و روی همین دستگاه ذخیره می‌ماند.</p>
          </div>
          <USwitch v-model="reducedFx" aria-label="جلوه‌های بصری کمتر" />
        </div>
      </GlassCard>

      <GlassCard data-reveal>
        <details :open="advancedOpen" @toggle="advancedOpen = ($event.target as HTMLDetailsElement).open">
          <summary class="flex cursor-pointer list-none items-center justify-between gap-3">
            <h2 class="display text-xl text-paper">تنظیمات پیشرفته</h2>
            <UIcon name="i-lucide-chevron-down" class="size-4 text-paper-mute transition-transform" :class="advancedOpen && 'rotate-180'" aria-hidden="true" />
          </summary>
          <div class="mt-5 space-y-4">
            <UFormField label="نشانی درگاه لاندا" :hint="`پیش‌فرض: ${DEFAULT_URL}`">
              <UInput v-model="form.landau_url" dir="ltr" class="w-full" />
            </UFormField>
            <UFormField label="آستانهٔ تطبیق خودکار" hint="از ۰ تا ۱۰۰؛ بالاتر یعنی محتاط‌تر. پیش‌فرض با آزمایش روی دادهٔ واقعی انتخاب شده">
              <UInput v-model.number="form.threshold" type="number" min="0" max="100" dir="ltr" class="w-32" />
            </UFormField>
          </div>
        </details>
      </GlassCard>

      <GlassCard data-reveal>
        <div class="mb-5 flex flex-wrap items-center justify-between gap-3">
          <h2 class="display text-xl text-paper">نسخهٔ برنامه</h2>
          <bdi v-if="upd" dir="ltr" class="tabular text-sm text-paper-dim">{{ faVersion(upd.current) }}</bdi>
        </div>
        <div class="flex items-center justify-between gap-6">
          <div>
            <div class="text-sm text-paper">بررسی و دریافت خودکار نسخهٔ جدید</div>
            <p class="text-xs text-paper-mute">هر چند ساعت از GitHub بررسی می‌شود و در پس‌زمینه دریافت می‌شود؛ هیچ‌وقت وسط یک عملیات نصب نمی‌شود. بدون اینترنت برنامه مثل همیشه کار می‌کند.</p>
          </div>
          <USwitch v-model="autoUpdate" :disabled="!upd" aria-label="بررسی و دریافت خودکار نسخهٔ جدید" />
        </div>
        <div v-if="upd && updLine" class="mt-5 flex flex-wrap items-center justify-between gap-3 border-t hairline pt-4">
          <div class="min-w-0">
            <StatusPill :tone="updLine.tone" :label="updLine.text" />
            <p class="mt-2 text-xs text-paper-mute">آخرین بررسی: {{ upd.last_check ? isoToLocal(upd.last_check) : 'هنوز نه' }}</p>
          </div>
          <div class="flex flex-wrap gap-2">
            <UButton v-if="canInstallNow(upd)" size="sm" icon="i-lucide-rotate-cw" :loading="updBusy === 'install'" :disabled="updBusy === 'check'" @click="installUpdate">نصب و راه‌اندازی دوباره</UButton>
            <UButton size="sm" color="neutral" variant="outline" icon="i-lucide-refresh-cw" :loading="updBusy === 'check'" :disabled="updBusy === 'install'" @click="checkUpdate">بررسی نسخهٔ جدید</UButton>
            <UButton size="sm" color="neutral" variant="ghost" icon="i-lucide-external-link" @click="openReleases">یادداشت‌های انتشار</UButton>
          </div>
        </div>
      </GlassCard>

      <GlassCard :pad="false" class="px-6 py-4" data-reveal>
        <div class="flex flex-wrap items-center justify-between gap-3">
          <div class="min-w-0">
            <div class="data-label mb-1">پوشهٔ داده</div>
            <button type="button" class="max-w-full cursor-pointer truncate font-mono text-xs text-paper-dim transition-colors hover:text-paper" dir="ltr" title="برای کپی کلیک کنید" @click="copyPath">{{ view?.data_dir ?? '—' }}</button>
          </div>
        </div>
        <p class="mt-3 text-xs" :class="view?.secret_backend === 'file' ? 'text-warn' : 'text-paper-mute'">
          {{ view?.secret_backend === 'keyring' ? 'رمزها در ذخیره‌ساز امن سیستم‌عامل نگه داشته می‌شوند.' : 'ذخیره‌ساز امن سیستم در دسترس نیست؛ رمزها در پرونده‌ای با دسترسی محدود در پوشهٔ داده نگه داشته می‌شوند.' }}
        </p>
      </GlassCard>
    </div>
  </div>
</template>
