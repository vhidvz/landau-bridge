<script setup lang="ts">
import type { SettingsView } from '~/types/bridge';

const props = defineProps<{ settings: SettingsView }>();
const steps = computed(() => [
  { label: 'کلید API لاندا', done: props.settings.has_landau_token, hint: 'در لاندا: حساب کاربری ← کلیدهای API؛ کلید فقط یک بار نمایش داده می‌شود' },
  { label: 'فروشگاه مقصد', done: !!props.settings.store, hint: 'بعد از «بررسی اتصال»، فروشگاهی را که ردیف‌ها روی قفسه‌اش نوشته می‌شود انتخاب کنید' },
  { label: 'ورود بارزپخش', done: !!props.settings.supplier_user && props.settings.has_supplier_password, hint: 'نام کاربری و کلمه عبور بارزپخش، سپس «آزمایش ورود»' },
]);
</script>

<template>
  <section class="surface flex flex-wrap items-center gap-8 p-8">
    <BrandLogo :size="96" class="shrink-0 text-primary" />
    <div class="min-w-0 flex-1">
      <h2 class="text-2xl">سه قدم تا اولین عبور</h2>
      <p class="mt-1 max-w-prose text-muted">پل وقتی باز می‌شود که هر دو کرانه وصل باشند.</p>
      <ol class="mt-5 space-y-3">
        <li v-for="(s, i) in steps" :key="s.label" class="flex items-start gap-3">
          <span class="check mt-0.5" :class="s.done && 'is-done'">
            <UIcon v-if="s.done" name="i-lucide-check" class="size-3.5" />
            <template v-else>{{ faNumber(i + 1) }}</template>
          </span>
          <span class="min-w-0">
            <span class="block font-medium text-highlighted">{{ s.label }}</span>
            <span class="block text-sm text-muted">{{ s.hint }}</span>
          </span>
        </li>
      </ol>
      <UButton to="/settings" class="mt-6" icon="i-lucide-settings">رفتن به تنظیمات</UButton>
    </div>
  </section>
</template>
