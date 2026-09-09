<script lang="ts">
  import MintForm from "./MintForm.svelte";

  let { onclose }: { onclose: () => void } = $props();

  let dlg = $state<HTMLDialogElement>();
  $effect(() => {
    dlg?.showModal();
  });
</script>

<dialog bind:this={dlg} onclose={onclose}>
  <div class="stack" style="width: min(440px, 90vw)">
    <div class="row between">
      <h3>Mint</h3>
      <span class="pill">blind · commit-reveal</span>
    </div>
    <MintForm ondone={() => dlg?.close()} />
  </div>
</dialog>
