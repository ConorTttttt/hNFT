<!--
  Deliberately NOT part of the main app's routing/component tree. A
  one-time, human-operated bootstrap step: a minter runs this once per
  identity (not per collection — the resulting X25519 keypair lives in
  their own lair keystore, not tied to any DNA) BEFORE creating the first
  collection that will run blind-box sales, then pastes the result into
  that collection's dna.yaml `modifiers.properties.minter_x25519_pubkey`.
  See docs/blind-box-collections.md.

  `generate_minter_x25519_pubkey` is an HDK call — it must run inside SOME
  already-installed cell's coordinator zome (any cell for this agent works,
  per the extern's own doc comment; it doesn't touch that cell's
  CollectionProperties). This page connects the same way the main app does
  (`lib/holochain.ts`) and calls the zome directly — it deliberately
  bypasses `NftClient`, since this isn't a runtime action any collection's
  UI should ever expose (see docs/blind-box-collections.md).
-->
<script lang="ts">
  import { connectHolochain } from "$lib/holochain";

  let pubkeyHex = $state("");
  let busy = $state(false);
  let error = $state("");

  async function generate() {
    busy = true;
    error = "";
    try {
      const client = await connectHolochain();
      // generate_minter_x25519_pubkey already returns a hex string — no
      // client-side encoding needed. role_name "nft" matches the default
      // NftClient uses; any already-installed cell for this agent works.
      pubkeyHex = await client.callZome({
        role_name: "nft",
        zome_name: "nft",
        fn_name: "generate_minter_x25519_pubkey",
        payload: null,
      });
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function copy() {
    await navigator.clipboard.writeText(pubkeyHex);
  }
</script>

<main class="stack" style="max-width: 640px; margin: 3rem auto; padding: 0 1rem">
  <h1>Generate your minter X25519 key</h1>
  <p>
    Run this once, before creating a collection that will run blind-box
    sales. Copy the result into that collection's <code>dna.yaml</code> as
    <code>modifiers.properties.minter_x25519_pubkey</code> — same manual
    step <code>minter_pubkey</code> already requires today.
  </p>
  <p class="faint small">
    The matching secret stays in this conductor's own lair keystore; it is
    never returned here and never leaves the conductor.
  </p>

  <button class="primary" onclick={generate} disabled={busy}>
    {busy ? "Generating…" : "Generate"}
  </button>

  {#if error}
    <p class="small" style="color: var(--danger, #c33)">{error}</p>
  {/if}

  {#if pubkeyHex}
    <div class="stack tight">
      <code class="mono" style="word-break: break-all">{pubkeyHex}</code>
      <button class="ghost tiny" onclick={copy}>Copy</button>
    </div>
  {/if}
</main>
