<script lang="ts">
  import {
    prepareCommitment,
    type NftAttribute,
  } from "@p2p-art-marketplace/client";
  import { useAppState } from "$lib/appState.svelte";

  let { ondone }: { ondone?: () => void } = $props();

  const app = useAppState();

  let attrs = $state<NftAttribute[]>([
    { trait_type: "name", value: "" },
    { trait_type: "", value: "" },
  ]);
  let imageUri = $state("");
  let busy = $state(false);

  const addRow = () => (attrs = [...attrs, { trait_type: "", value: "" }]);
  const removeRow = (i: number) => (attrs = attrs.filter((_, j) => j !== i));

  async function mint() {
    busy = true;
    try {
      const cleaned = attrs
        .map((a) => ({ trait_type: a.trait_type.trim(), value: a.value.trim() }))
        .filter((a) => a.trait_type !== "" && a.value !== "");
      await app.mint(prepareCommitment(cleaned, imageUri.trim()));
      ondone?.();
    } finally {
      busy = false;
    }
  }
</script>

<div class="stack">
  {#if !app.isMinter}
    <p class="small muted">
      This collection restricts minting to its configured minter — your mint will
      be rejected.
    </p>
  {/if}

  <label class="field">
    <span>Image URI</span>
    <input bind:value={imageUri} placeholder="https://…  ·  ipfs://…  ·  data:…" />
  </label>

  <div class="field">
    <span>Attributes — a <code>name</code> attribute becomes the title</span>
    <div class="stack tight">
      {#each attrs as attr, i (i)}
        <div class="row">
          <input class="grow" bind:value={attr.trait_type} placeholder="trait" />
          <input class="grow" bind:value={attr.value} placeholder="value" />
          <button
            class="ghost tiny"
            onclick={() => removeRow(i)}
            disabled={attrs.length === 1}
            aria-label="remove">✕</button
          >
        </div>
      {/each}
      <div><button class="ghost tiny" onclick={addRow}>+ attribute</button></div>
    </div>
  </div>

  <p class="small faint">
    The salt is generated now and saved to this browser — you need it to reveal.
    The metadata stays sealed until you reveal it.
  </p>

  <div class="row between">
    <button class="ghost" onclick={() => ondone?.()}>Cancel</button>
    <button class="primary" onclick={mint} disabled={busy}>
      {busy ? "Minting…" : "Mint sealed NFT"}
    </button>
  </div>
</div>
