<script lang="ts">
  import { useAppState } from "$lib/appState.svelte";
  import { b64, eqHash, shortHash } from "$lib/format";
  import GenerativeStatusPanel from "./GenerativeStatusPanel.svelte";

  /**
   * Sealed NFTs this agent minted in a GENERATIVE collection. Deliberately
   * separate from `DraftShelf` (which is driven by `saltStore`'s client-held
   * salt/attributes — a mechanism that doesn't apply here: a generative NFT's
   * traits are protocol-derived, never client-held). Sourced from `app.myNfts`
   * instead, since a self-minted, not-yet-transferred NFT is already indexed
   * there by `AgentToNft`.
   */
  const app = useAppState();

  const sealedOwnMints = $derived(
    app.myNfts.filter(
      (v) => v.manifest.payload.status === "sealed" && eqHash(v.minter, app.me),
    ),
  );
</script>

{#if sealedOwnMints.length}
  <div class="panel stack">
    <div class="row between">
      <h3>Generative NFTs awaiting reveal <span class="faint">· {sealedOwnMints.length}</span></h3>
    </div>
    <div class="stack tight">
      {#each sealedOwnMints as v (b64(v.nft))}
        <div class="draft">
          <GenerativeStatusPanel nft={v.nft} title={shortHash(v.nft)} />
        </div>
      {/each}
    </div>
  </div>
{/if}
