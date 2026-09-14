/**
 * `v-reveal` on a page's root: children marked `data-reveal` rise into place, staggered 70 ms.
 * CSS-only (main.css `.reveal-armed`); under reduced motion or fx-lite everything shows at once.
 */
export default defineNuxtPlugin((nuxtApp) => {
  nuxtApp.vueApp.directive('reveal', {
    beforeMount(el: HTMLElement) {
      el.classList.add('reveal-armed');
    },
    mounted(el: HTMLElement) {
      const targets = el.querySelectorAll<HTMLElement>('[data-reveal]');
      if (prefersReducedMotion() || !targets.length) {
        el.classList.add('is-revealed');
        return;
      }
      targets.forEach((t, i) => t.style.setProperty('--reveal-delay', `${i * 70}ms`));
      // two frames: let the hidden state paint once so the transition has a start value
      requestAnimationFrame(() => requestAnimationFrame(() => el.classList.add('is-revealed')));
    },
  });
});
