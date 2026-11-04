<script setup lang="ts">
// Every table's frame: the table in a scroll box that never grows past twice the window's height
// (`.ledger`, main.css), its header pinned while that box scrolls, and the pager as its footer.
// A page is sized to fit (PAGE_SIZE); the cap holds when rows grow anyway — a long note, an
// opened error's details — so a table never becomes a wall to scroll past.
defineSlots<{ default(): unknown; footer?(): unknown }>();
</script>

<template>
  <div class="ledger-frame" data-paged>
    <div class="ledger">
      <table>
        <slot />
      </table>
    </div>
    <div v-if="$slots.footer" class="ledger-foot">
      <slot name="footer" />
    </div>
  </div>
</template>
