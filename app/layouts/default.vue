<script setup lang="ts">
import type { JobRow, SettingsView } from '~/types/bridge';

const links = [
  { to: '/', label: 'داشبورد', icon: 'i-lucide-layout-dashboard' },
  { to: '/import', label: 'وارد کردن فاکتورها', icon: 'i-lucide-file-input' },
  { to: '/refresh', label: 'به‌روزرسانی قیمت‌ها', icon: 'i-lucide-refresh-cw' },
  { to: '/matches', label: 'تطبیق‌ها', icon: 'i-lucide-link' },
  { to: '/jobs', label: 'عملیات‌ها', icon: 'i-lucide-history' },
  { to: '/errors', label: 'خطاها و هشدارها', icon: 'i-lucide-triangle-alert' },
  { to: '/settings', label: 'تنظیمات', icon: 'i-lucide-settings' },
];
const route = useRoute();
const isActive = (to: string) => (to === '/' ? route.path === '/' : route.path.startsWith(to));

const running = ref<JobRow | null>(null);
const settings = ref<SettingsView | null>(null);
const savedTick = useSettingsSaved();
const complete = computed(() =>
  !!settings.value && settings.value.has_landau_token && !!settings.value.store && !!settings.value.supplier_user && settings.value.has_supplier_password);

const poll = async () => { running.value = await bridge.jobCurrent().catch(() => null); };
const loadSettings = async () => { settings.value = await bridge.settingsGet().catch(() => null); };
let timer: ReturnType<typeof setInterval> | undefined;
onMounted(() => { poll(); loadSettings(); timer = setInterval(poll, 3000); });
onBeforeUnmount(() => clearInterval(timer));
watch(savedTick, loadSettings);
</script>

<template>
  <div class="flex h-screen overflow-hidden">
    <GlowBackdrop />

    <aside class="flex w-[240px] shrink-0 flex-col border-e hairline bg-ink-900/40 backdrop-blur-xl" aria-label="بخش‌های برنامه">
      <NuxtLink to="/" class="flex items-center gap-3 px-5 pt-6 pb-8">
        <span class="brand-tile"><BrandLogo :size="20" tone="mono" decorative /></span>
        <span class="leading-tight">
          <span class="display block text-[1.05rem] text-paper">پل لاندا</span>
          <span class="block text-[0.6875rem] text-paper-mute">بارزپخش ← لاندا</span>
        </span>
      </NuxtLink>

      <nav class="flex-1 space-y-1 px-3" aria-label="صفحه‌ها">
        <NuxtLink v-for="l in links" :key="l.to" :to="l.to" class="nav-item" :class="isActive(l.to) && 'nav-active'">
          <UIcon :name="l.icon" class="size-[17px] shrink-0" aria-hidden="true" />
          <span>{{ l.label }}</span>
        </NuxtLink>
      </nav>

      <div class="mt-auto flex flex-col gap-3 px-5 pb-5">
        <RunningJobCard v-if="running" :job="running" compact />
        <div v-else class="glass-inset flex items-center gap-2.5 px-3.5 py-3">
          <span class="dot" :class="complete ? 'bg-ok text-ok' : 'bg-warn text-warn'" aria-hidden="true" />
          <div class="min-w-0 leading-tight">
            <div class="text-xs font-medium text-paper-dim">{{ complete ? 'هر دو کرانه وصل است' : 'تنظیمات ناقص' }}</div>
            <div class="truncate text-[0.6875rem] text-paper-mute">
              <template v-if="complete">{{ settings?.store_name ?? settings?.store }}</template>
              <NuxtLink v-else to="/settings" class="hover:text-crimson-300">رفتن به تنظیمات ←</NuxtLink>
            </div>
          </div>
        </div>
      </div>
    </aside>

    <main class="min-w-0 flex-1 overflow-y-auto">
      <div class="mx-auto max-w-[1060px] px-10 pt-10 pb-20">
        <slot />
      </div>
    </main>
  </div>
</template>
