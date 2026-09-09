<script lang="ts">
  import { identiconSvg } from "$lib/identicon";

  let { seed, size = 24 }: { seed: Uint8Array; size?: number } = $props();

  // The SVG is generated purely from the key bytes — no external input.
  const svg = $derived(identiconSvg(seed, size));
</script>

<span
  class="identicon"
  style="width:{size}px;height:{size}px"
>{@html svg}</span>

<style>
  .identicon {
    display: inline-flex;
    border-radius: 6px;
    overflow: hidden;
    flex: none;
  }
</style>
