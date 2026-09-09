<script lang="ts">
  import {
    prepareCommitment,
    type NftAttribute,
  } from "@p2p-art-marketplace/client";
  import { getAppStateContext } from "$lib/context";
  import { b64 } from "$lib/format";

  const app = getAppStateContext();

  let attrs = $state<NftAttribute[]>([{ trait_type: "", value: "" }]);
  let imageUri = $state("");
  let busy = $state(false);
  let err = $state("");
  let mintedB64 = $state("");

  function addRow() {
    attrs = [...attrs, { trait_type: "", value: "" }];
  }
  function removeRow(i: number) {
    attrs = attrs.filter((_, j) => j !== i);
  }

  async function mint() {
    busy = true;
    err = "";
    mintedB64 = "";
    try {
      const cleaned = attrs
        .map((a) => ({ trait_type: a.trait_type.trim(), value: a.value.trim() }))
        .filter((a) => a.trait_type !== "");
      const commitment = prepareCommitment(cleaned, imageUri.trim());
      const nft = await app.mint(commitment);
      mintedB64 = b64(nft);
      attrs = [{ trait_type: "", value: "" }];
      imageUri = "";
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="panel stack">
  <h3 style="margin: 0">Mint (blind)</h3>
  {#if !app.isMinter}
    <p class="muted">
      This collection restricts minting to its configured minter — your mint will
      be rejected.
    </p>
  {/if}

  <label class="stack" style="gap: 0.25rem">
    <span class="muted">Image URI</span>
    <input bind:value={imageUri} placeholder="ipfs://…" />
  </label>

  <div class="stack" style="gap: 0.4rem">
    <span class="muted">Attributes</span>
    {#each attrs as attr, i (i)}
      <div class="row">
        <input
          style="flex: 1"
          bind:value={attr.trait_type}
          placeholder="trait_type"
        />
        <input style="flex: 1" bind:value={attr.value} placeholder="value" />
        <button
          class="ghost"
          onclick={() => removeRow(i)}
          disabled={attrs.length === 1}>✕</button
        >
      </div>
    {/each}
    <button class="ghost" onclick={addRow}>+ attribute</button>
  </div>

  <div class="row">
    <button class="primary" onclick={mint} disabled={busy}>
      {busy ? "Minting…" : "Mint"}
    </button>
    <span class="muted">
      The salt is generated now and saved to this browser — keep it to reveal
      later.
    </span>
  </div>

  {#if err}<p class="error">{err}</p>{/if}
  {#if mintedB64}
    <p>
      Minted <span class="mono">{mintedB64}</span> — sealed. Reveal it from its
      detail view.
    </p>
  {/if}
</div>
