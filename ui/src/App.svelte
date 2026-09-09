<script lang="ts">
  import { AppState, setCurrentAppState } from "$lib/appState.svelte";
  import { connectHolochain } from "$lib/holochain";
  import { b64 } from "$lib/format";
  import MintForm from "./components/MintForm.svelte";
  import NftDetail from "./components/NftDetail.svelte";
  import OfferInbox from "./components/OfferInbox.svelte";
  import WalletGrid from "./components/WalletGrid.svelte";

  let phase = $state<"connecting" | "ready" | "error">("connecting");
  let errMsg = $state("");
  let app = $state<AppState | null>(null);
  let tab = $state<"wallet" | "mint">("wallet");

  $effect(() => {
    let s: AppState | null = null;
    let cancelled = false;
    (async () => {
      try {
        const client = await connectHolochain();
        s = new AppState(client, client.myPubKey);
        if (cancelled) {
          s.dispose();
          return;
        }
        setCurrentAppState(s);
        app = s;
        await s.init();
        if (!cancelled) phase = "ready";
      } catch (e) {
        if (!cancelled) {
          errMsg = String(e);
          phase = "error";
        }
      }
    })();
    return () => {
      cancelled = true;
      s?.dispose();
      setCurrentAppState(null);
    };
  });
</script>

{#if phase === "connecting"}
  <main style="padding: 2rem"><p class="muted">Connecting to the conductor…</p></main>
{:else if phase === "error"}
  <main style="padding: 2rem">
    <p class="error">Could not connect: {errMsg}</p>
    <p class="muted">
      Run the UI via <span class="mono">hc-spin</span>, or set
      <span class="mono">VITE_HC_ADMIN_PORT</span> for a packaged conductor.
    </p>
  </main>
{:else if app}
  <header
    class="row"
    style="justify-content: space-between; padding: 0.75rem 1rem; border-bottom: 1px solid var(--border)"
  >
    <strong>
      {app.collection?.name ?? "Collection"}
      {#if app.collection}<span class="muted">({app.collection.symbol})</span>{/if}
    </strong>
    <span class="mono muted" title="your agent key">{b64(app.me)}</span>
  </header>

  {#if app.error}
    <p class="error" style="padding: 0 1rem">{app.error}</p>
  {/if}

  <main
    style="display: grid; grid-template-columns: 1fr 320px; gap: 1rem; padding: 1rem; align-items: start"
  >
    <section class="stack">
      <div class="row">
        <button class:primary={tab === "wallet"} onclick={() => (tab = "wallet")}>
          Wallet
        </button>
        <button class:primary={tab === "mint"} onclick={() => (tab = "mint")}>
          Mint
        </button>
      </div>
      {#if tab === "wallet"}
        <WalletGrid />
      {:else}
        <MintForm />
      {/if}
    </section>

    <aside><OfferInbox /></aside>
  </main>

  {#if app.detailNft}
    <NftDetail />
  {/if}
{/if}
