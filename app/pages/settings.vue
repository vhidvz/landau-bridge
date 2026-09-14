<script setup lang="ts">
import type { BridgeError, LandauStatus, SettingsView } from '~/types/bridge';

const toast = useToast();
const view = ref<SettingsView | null>(null);
const error = ref<BridgeError | null>(null);
const landau = ref<LandauStatus | null>(null);
const busy = ref<'landau' | 'supplier' | 'save' | null>(null);

const supplierOk = ref(false);
const landauChip = computed(() =>
  landau.value ? { label: `متصل — ${landau.value.business_name ?? landau.value.business}`, color: 'success' as const }
    : view.value?.has_landau_token ? { label: 'کلید ذخیره‌شده', color: 'neutral' as const }
      : { label: 'آزمایش‌نشده', color: 'warning' as const });
const supplierChip = computed(() =>
  supplierOk.value ? { label: 'ورود موفق', color: 'success' as const }
    : view.value?.has_supplier_password ? { label: 'رمز ذخیره‌شده', color: 'neutral' as const }
      : { label: 'آزمایش‌نشده', color: 'warning' as const });

const form = reactive({
  landau_url: '', landau_token: '', store: '', supplier_user: '', supplier_password: '',
  default_authenticity: 'ORIGINAL' as 'ORIGINAL' | 'FAKE', threshold: 80, vat_percent: 10, overwrite_manual: false,
});

const load = async () => {
  try {
    view.value = await bridge.settingsGet();
    Object.assign(form, {
      landau_url: view.value.landau_url, store: view.value.store ?? '', supplier_user: view.value.supplier_user ?? '',
      default_authenticity: view.value.default_authenticity, threshold: view.value.threshold, vat_percent: view.value.vat_percent,
      overwrite_manual: view.value.overwrite_manual, landau_token: '', supplier_password: '',
    });
  } catch (e) { error.value = e as BridgeError; }
};
onMounted(load);

const save = async () => {
  error.value = null; busy.value = 'save';
  try {
    const storeName = landau.value?.stores.find((s) => s.id === form.store)?.name ?? view.value?.store_name ?? undefined;
    view.value = await bridge.settingsSave({ ...form, store_name: storeName ?? undefined });
    form.landau_token = ''; form.supplier_password = '';
    toast.add({ title: 'تنظیمات ذخیره شد', color: 'success' });
  } catch (e) { error.value = e as BridgeError; } finally { busy.value = null; }
};

const validateLandau = async () => {
  error.value = null; busy.value = 'landau';
  try {
    // save url+key first so the check uses what is typed
    view.value = await bridge.settingsSave({ landau_url: form.landau_url, landau_token: form.landau_token || undefined });
    form.landau_token = '';
    landau.value = await bridge.landauValidate();
    if (!form.store && landau.value.stores.length === 1) form.store = landau.value.stores[0]!.id;
    toast.add({ title: `اتصال به لندو برقرار است — ${landau.value.business_name ?? landau.value.business}`, color: 'success' });
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

const storeItems = computed(() => (landau.value?.stores ?? []).map((s) => ({ label: s.name ?? s.id, value: s.id })));
const authItems = [{ label: 'اصلی (ORIGINAL)', value: 'ORIGINAL' }, { label: 'غیراصلی / کپی (FAKE)', value: 'FAKE' }];
</script>

<template>
  <div class="mx-auto max-w-3xl">
    <PageHeader title="تنظیمات" description="اتصال‌ها و قواعد وارد کردن." />
    <ErrorBanner :error="error" />

    <div class="surface divide-y divide-default">
      <section class="p-6">
        <div class="mb-4 flex items-center justify-between gap-3">
          <h2>لندو</h2>
          <UBadge :color="landauChip.color" variant="soft">{{ landauChip.label }}</UBadge>
        </div>
        <div class="space-y-4">
          <UFormField label="نشانی درگاه لندو" hint="مثال: https://api.landau.ir">
            <UInput v-model="form.landau_url" dir="ltr" class="w-full" />
          </UFormField>
          <UFormField label="کلید API لندو" :hint="view?.has_landau_token ? 'کلیدی ذخیره شده است؛ برای تغییر، کلید تازه را وارد کنید' : 'در لندو: حساب کاربری ← کلیدهای API'">
            <UInput v-model="form.landau_token" type="password" dir="ltr" placeholder="apt-…" class="w-full" />
          </UFormField>
          <UButton :loading="busy === 'landau'" variant="soft" icon="i-lucide-plug-zap" @click="validateLandau">بررسی اتصال و دریافت فروشگاه‌ها</UButton>
          <p v-if="landau && landau.active_rules === 0" class="text-sm text-warning">هیچ قاعدهٔ قیمت‌گذاری فعالی ندارید؛ تا زمانی که در لندو قاعده‌ای بسازید، قیمت قفسه برابر قیمت خرید خواهد بود.</p>
          <p v-else-if="landau" class="tabular text-sm text-muted">{{ faNumber(landau.active_rules) }} قاعدهٔ قیمت‌گذاری فعال</p>
          <UFormField label="فروشگاه مقصد" hint="ردیف‌ها روی قفسهٔ این فروشگاه نوشته می‌شوند">
            <USelect v-model="form.store" :items="storeItems" :disabled="!storeItems.length" placeholder="ابتدا اتصال را بررسی کنید" class="w-full" />
            <p v-if="!storeItems.length && view?.store_name" class="mt-1 text-sm text-muted">فروشگاه ذخیره‌شده: {{ view.store_name }}</p>
          </UFormField>
        </div>
      </section>

      <section class="p-6">
        <div class="mb-4 flex items-center justify-between gap-3">
          <h2>بارزپخش</h2>
          <UBadge :color="supplierChip.color" variant="soft">{{ supplierChip.label }}</UBadge>
        </div>
        <div class="space-y-4">
          <UFormField label="نام کاربری"><UInput v-model="form.supplier_user" dir="ltr" class="w-full" /></UFormField>
          <UFormField label="کلمه عبور" :hint="view?.has_supplier_password ? 'ذخیره شده؛ برای تغییر، مقدار تازه را وارد کنید' : ''">
            <UInput v-model="form.supplier_password" type="password" dir="ltr" class="w-full" />
          </UFormField>
          <UButton :loading="busy === 'supplier'" variant="soft" icon="i-lucide-log-in" @click="testSupplier">آزمایش ورود</UButton>
        </div>
      </section>

      <section class="p-6">
        <h2 class="mb-4">قواعد وارد کردن</h2>
        <div class="space-y-4">
          <UFormField label="اصالت پیش‌فرض ردیف‌های تازه" hint="وضعیت همیشه «نو» ثبت می‌شود؛ اصالت را می‌توانید برای هر قلم در مرحلهٔ بازبینی تغییر دهید">
            <USelect v-model="form.default_authenticity" :items="authItems" class="w-full" />
          </UFormField>
          <div class="grid gap-4 sm:grid-cols-2">
            <UFormField label="آستانهٔ تطبیق خودکار" hint="از ۰ تا ۱۰۰؛ بالاتر یعنی محتاط‌تر. پیش‌فرض با آزمایش روی دادهٔ واقعی انتخاب شده">
              <UInput v-model.number="form.threshold" type="number" min="0" max="100" dir="ltr" class="w-32" />
            </UFormField>
            <UFormField label="نرخ ارزش افزوده (٪)" hint="قیمت خرید = قیمت فهرست بارزپخش × (۱ + نرخ)">
              <UInput v-model.number="form.vat_percent" type="number" min="0" max="100" step="0.5" dir="ltr" class="w-32" />
            </UFormField>
          </div>
          <UFormField label="قیمت‌های دستی لندو را هم بازنویسی کن" hint="لندو قیمتی را که خودتان تایپ کرده باشید نگه می‌دارد؛ این گزینه همان «بازنویسی» لندوست">
            <USwitch v-model="form.overwrite_manual" />
          </UFormField>
        </div>
      </section>

      <section class="p-6 text-sm">
        <h2 class="mb-3">داده‌ها</h2>
        <p>پوشهٔ داده: <code dir="ltr">{{ view?.data_dir }}</code></p>
        <p class="mt-2" :class="view?.secret_backend === 'file' ? 'text-warning' : 'text-muted'">
          {{ view?.secret_backend === 'keyring' ? 'رمزها در ذخیره‌ساز امن سیستم‌عامل نگه داشته می‌شوند.' : 'ذخیره‌ساز امن سیستم در دسترس نیست؛ رمزها در پرونده‌ای با دسترسی محدود در پوشهٔ داده نگه داشته می‌شوند.' }}
        </p>
      </section>
    </div>

    <div class="sticky bottom-0 mt-6 flex justify-end border-t border-default bg-muted/85 py-3 backdrop-blur">
      <UButton size="lg" :loading="busy === 'save'" icon="i-lucide-save" @click="save">ذخیره تنظیمات</UButton>
    </div>
  </div>
</template>
