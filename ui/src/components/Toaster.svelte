<script lang="ts">
  import { toasts, dismissToast } from "$lib/toast.svelte";
</script>

<div class="toaster">
  {#each toasts as t (t.id)}
    <div class="toast {t.kind}" role="status">
      <div class="row between">
        <span class="grow">{t.message}</span>
        <button class="ghost tiny" onclick={() => dismissToast(t.id)}>✕</button>
      </div>
    </div>
  {/each}
</div>
