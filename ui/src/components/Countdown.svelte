<script lang="ts">
  import { formatCountdown } from "$lib/format";

  let { deadlineMs, onexpire }: { deadlineMs: number; onexpire?: () => void } =
    $props();

  let now = $state(Date.now());
  let fired = false;

  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(id);
  });

  const remaining = $derived(deadlineMs - now);

  $effect(() => {
    if (remaining <= 0 && !fired) {
      fired = true;
      onexpire?.();
    }
  });
</script>

<span class="mono" class:error={remaining <= 0}>{formatCountdown(remaining)}</span>
