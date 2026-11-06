<script lang="ts">
  /** Sudo's silver capsule (the app icon), drawn as a vector so it stays crisp at any size. */
  let { size = 92 }: { size?: number } = $props();
</script>

<svg class="logo" width={size} height={size} viewBox="0 0 92 92" role="img" aria-label="Sudo">
  <defs>
    <linearGradient id="sudo-silver" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#ffffff" />
      <stop offset="0.45" stop-color="#b9bec8" />
      <stop offset="0.7" stop-color="#e9ecf1" />
      <stop offset="1" stop-color="#8d929c" />
    </linearGradient>
  </defs>
  <rect x="10" y="31" width="72" height="30" rx="15" fill="none" stroke="url(#sudo-silver)" stroke-width="4.2" />
</svg>

<style>
  .logo { display: block; filter: drop-shadow(0 2px 6px oklch(0.1 0 0 / 0.35)); }
</style>
