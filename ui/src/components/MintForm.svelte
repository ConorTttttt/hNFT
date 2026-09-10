<script lang="ts">
  import {
    prepareCommitment,
    type NftAttribute,
  } from "@p2p-art-marketplace/client";
  import { useAppState } from "$lib/appState.svelte";
  import { unb64 } from "$lib/format";

  let { ondone }: { ondone?: () => void } = $props();

  const app = useAppState();

  let attrs = $state<NftAttribute[]>([
    { trait_type: "name", value: "" },
    { trait_type: "", value: "" },
  ]);
  let imageUri = $state("");
  let busy = $state(false);

  // --- optional reveal agreement ---
  let useAgreement = $state(false);
  let revealByDays = $state(7);
  let defaultAfterExtraDays = $state(7);
  let delegateKey = $state("");
  let autoReveal = $state(true);

  const addRow = () => (attrs = [...attrs, { trait_type: "", value: "" }]);
  const removeRow = (i: number) => (attrs = attrs.filter((_, j) => j !== i));

  async function mint() {
    busy = true;
    try {
      const cleaned = attrs
        .map((a) => ({ trait_type: a.trait_type.trim(), value: a.value.trim() }))
        .filter((a) => a.trait_type !== "" && a.value !== "");
      const commitment = prepareCommitment(cleaned, imageUri.trim());

      if (useAgreement) {
        const revealBy = new Date(Date.now() + revealByDays * 86_400_000);
        const defaultAfter = new Date(
          revealBy.getTime() + Math.max(1, defaultAfterExtraDays) * 86_400_000,
        );
        const delegate = delegateKey.trim() ? unb64(delegateKey.trim()) : undefined;
        await app.mint(commitment, { revealBy, defaultAfter, delegate, autoReveal });
      } else {
        await app.mint(commitment);
      }
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

  <details bind:open={useAgreement} class="agreement">
    <summary>Reveal agreement <span class="faint small">(optional)</span></summary>
    <div class="stack tight" style="margin-top: 0.5rem">
      <label class="field">
        <span>Reveal within (days)</span>
        <input type="number" min="1" bind:value={revealByDays} />
      </label>
      <label class="field">
        <span>Grace after that before it can be marked defaulted (days)</span>
        <input type="number" min="1" bind:value={defaultAfterExtraDays} />
      </label>
      <label class="field">
        <span>Delegate agent key (optional) — may reveal after the deadline</span>
        <input class="mono" bind:value={delegateKey} placeholder="uhCAk…" />
      </label>
      <label class="row" style="gap: 0.5rem">
        <input
          type="checkbox"
          bind:checked={autoReveal}
          style="width: auto"
        />
        <span class="small">
          Auto-reveal — park the secret privately on-chain so this conductor
          reveals it once the deadline passes
        </span>
      </label>
    </div>
  </details>

  <p class="small faint">
    The salt is generated now and saved to this browser — you need it to reveal.
    {#if !useAgreement}The metadata stays sealed until you reveal it.{/if}
  </p>

  <div class="row between">
    <button class="ghost" onclick={() => ondone?.()}>Cancel</button>
    <button class="primary" onclick={mint} disabled={busy}>
      {busy ? "Minting…" : useAgreement ? "Mint with agreement" : "Mint sealed NFT"}
    </button>
  </div>
</div>

<style>
  .agreement summary {
    cursor: pointer;
    color: var(--text-dim);
    font-size: 0.85rem;
  }
</style>
