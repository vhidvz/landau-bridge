<script setup lang="ts">
import type { JobEvent, JobEventRow } from '~/types/bridge';

type AnyEvent = JobEvent | JobEventRow;
const props = defineProps<{ events: AnyEvent[]; kind: 'IMPORT' | 'REFRESH'; limit?: number }>();
const shown = computed(() => (props.limit ? props.events.slice(0, props.limit) : props.events));
// live `JobEvent`s carry no timestamp; stored rows do
const at = (e: AnyEvent): string | null => ('at' in e && e.at ? isoToLocal(e.at) : null);
const keyOf = (e: AnyEvent, i: number) => ('id' in e ? e.id : i);
</script>

<template>
  <ul class="divide-y divide-white/5">
    <li v-for="(e, i) in shown" :key="keyOf(e, i)" class="flex items-start gap-3 px-6 py-3">
      <span class="level-dot" :data-level="e.level" aria-hidden="true" />
      <div class="min-w-0 flex-1">
        <p class="tabular text-xs text-paper-mute">
          <template v-if="at(e)">{{ at(e) }} · </template>
          <template v-if="phaseLabel[e.phase]">{{ phaseLabel[e.phase] }}</template>
          <bdi v-else dir="ltr">{{ e.phase }}</bdi>
        </p>
        <p class="text-sm break-words" :class="e.level === 'ERROR' ? 'text-danger' : e.level === 'WARN' ? 'text-warn' : 'text-paper-dim'">{{ reportEventText(e, kind) }}</p>
        <details v-if="e.detail != null" class="mt-1.5">
          <summary class="cursor-pointer text-xs text-paper-mute hover:text-paper-dim">جزئیات فنی</summary>
          <pre class="code-block mt-2" dir="ltr">{{ JSON.stringify(e.detail, null, 1) }}</pre>
        </details>
      </div>
    </li>
  </ul>
</template>
