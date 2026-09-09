<script lang="ts">
  import {
    prepareCommitment,
    type NftAttribute,
  } from "@p2p-art-marketplace/client";
  import { useAppState } from "$lib/appState.svelte";

  const app = useAppState();

  let attrs = $state<NftAttribute[]>([{ trait_type: "", value: "" }]);
  let imageUri = $state("");
  let busy = $state(false);

  const addRow = () => (attrs = [...attrs, { trait_type: "", value: "" }]);
  const removeRow = (i: number) => (attrs = attrs.filter((_, j) => j !== i));

  async function mint() {
    busy = true;
    try {
      const cleaned = attrs
        .map((a) => ({ trait_type: a.trait_type.trim(), value: a.value.trim() }))
        .filter((a) => a.trait_type !== "");
      await app.mint(prepareCommitment(cleaned, imageUri.trim()));
      attrs = [{ trait_type: "", value: "" }];
      imageUri = "";
    } finally {
      busy = false;
    }
  }
</script>

<div class="panel stack">
  <div class="row between">
    <h3>Mint</h3>
    <span class="pill">blind · commit-reveal</span>
  </div>

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
    <span>Attributes</span>
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

  <div class="divider"></div>

  <div class="row between wrap">
    <span class="small faint">
      The salt is generated now and saved to this browser — you need it to reveal.
    </span>
    <button class="primary" onclick={mint} disabled={busy}>
      {busy ? "Minting…" : "Mint sealed"}
    </button>
  </div>
</div>
