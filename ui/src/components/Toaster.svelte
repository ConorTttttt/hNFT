<script lang="ts">
  import { toasts, dismissToast } from "$lib/toast.svelte";

  function activate(id: number, fn?: () => void) {
    fn?.();
    dismissToast(id);
  }
</script>

<div class="toaster">
  {#each toasts as t (t.id)}
    <div class="toast {t.kind}" role="status" class:clickable={t.onClick}>
      <div class="row between">
        {#if t.onClick}
          <button class="link grow" onclick={() => activate(t.id, t.onClick)}>
            {t.message} →
          </button>
        {:else}
          <span class="grow">{t.message}</span>
        {/if}
        <button class="ghost tiny" onclick={() => dismissToast(t.id)}>✕</button>
      </div>
    </div>
  {/each}
</div>

<style>
  .toast.clickable {
    cursor: pointer;
  }
  .link {
    background: none;
    border: none;
    color: inherit;
    text-align: left;
    padding: 0;
    font: inherit;
    cursor: pointer;
  }
  .link:hover {
    color: var(--accent-2);
  }
</style>
