<script lang="ts">
  import {
    prepareCommitment,
    type NftAttribute,
  } from "@p2p-art-marketplace/client";
  import { useAppState } from "$lib/appState.svelte";
  import { unb64 } from "$lib/format";

  let { ondone }: { ondone?: () => void } = $props();

  const app = useAppState();

  /** Mirrors the authoritative `mint_status()` picture — never a second
   * authorization decision. If a race lets this warning be stale, the
   * protocol still rejects the mint and `app.mint`'s error toast surfaces it. */
  const unavailableReason = $derived.by(() => {
    if (app.isMinter) return null;
    const s = app.mintStatus;
    if (!s) return "This collection restricts who can mint — your mint will be rejected.";
    if (s.policy === "whitelist") {
      return s.my_allowance == null
        ? "You are not on this collection's mint whitelist — your mint will be rejected."
        : "You've reached your whitelist mint allowance — your mint will be rejected.";
    }
    return "This collection restricts minting to its configured minter — your mint will be rejected.";
  });

  let attrs = $state<NftAttribute[]>([
    { trait_type: "name", value: "" },
    { trait_type: "", value: "" },
  ]);
  let imageUri = $state("");
  let busy = $state(false);

  // --- optional reveal agreement ---
  const UNIT_MS = { seconds: 1_000, minutes: 60_000, hours: 3_600_000, days: 86_400_000 } as const;
  type TimeUnit = keyof typeof UNIT_MS;

  let useAgreement = $state(false);
  let revealByValue = $state(7);
  let revealByUnit = $state<TimeUnit>("days");
  let defaultAfterValue = $state(7);
  let defaultAfterUnit = $state<TimeUnit>("days");
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
        const revealBy = new Date(Date.now() + Math.max(1, revealByValue) * UNIT_MS[revealByUnit]);
        const defaultAfter = new Date(
          revealBy.getTime() + Math.max(1, defaultAfterValue) * UNIT_MS[defaultAfterUnit],
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
  {#if unavailableReason}
    <p class="small muted">{unavailableReason}</p>
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
        <span>Reveal within</span>
        <div class="row" style="gap: 0.4rem">
          <input type="number" min="1" style="max-width: 6rem" bind:value={revealByValue} />
          <select bind:value={revealByUnit}>
            <option value="seconds">seconds</option>
            <option value="minutes">minutes</option>
            <option value="hours">hours</option>
            <option value="days">days</option>
          </select>
        </div>
      </label>
      <label class="field">
        <span>Grace after that before it can be marked defaulted</span>
        <div class="row" style="gap: 0.4rem">
          <input type="number" min="1" style="max-width: 6rem" bind:value={defaultAfterValue} />
          <select bind:value={defaultAfterUnit}>
            <option value="seconds">seconds</option>
            <option value="minutes">minutes</option>
            <option value="hours">hours</option>
            <option value="days">days</option>
          </select>
        </div>
      </label>
      {#if revealByUnit !== "days" || defaultAfterUnit !== "days"}
        <p class="faint small">
          Short windows are handy for testing live, but the auto-reveal sweep
          only checks every 5 minutes — a manual Reveal (or a delegate's,
          once the deadline passes) shows the deadline gate working
          immediately either way.
        </p>
      {/if}
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
