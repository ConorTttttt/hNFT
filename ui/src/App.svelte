<script lang="ts">
  import { onMount } from "svelte";
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

  function withTimeout<T>(p: Promise<T>, ms: number, label: string): Promise<T> {
    return Promise.race([
      p,
      new Promise<T>((_, reject) =>
        setTimeout(() => reject(new Error(`${label} timed out after ${ms}ms`)), ms),
      ),
    ]);
  }

  async function connect() {
    phase = "connecting";
    errMsg = "";
    app?.dispose();
    app = null;
    setCurrentAppState(null);
    try {
      const client = await withTimeout(
        connectHolochain(),
        15000,
        "conductor connection",
      );
      const s = new AppState(client, client.myPubKey);
      setCurrentAppState(s);
      app = s;
      phase = "ready"; // show the UI as soon as the client is up …
      s.init(); // … and let the stores populate with their own loading/error state
    } catch (e) {
      errMsg = String(e);
      phase = "error";
    }
  }

  onMount(() => {
    connect();
    return () => {
      app?.dispose();
      setCurrentAppState(null);
    };
  });
</script>

{#if phase === "connecting"}
  <main style="padding: 2rem"><p class="muted">Connecting to the conductor…</p></main>
{:else if phase === "error"}
  <main style="padding: 2rem" class="stack">
    <p class="error">Could not connect: {errMsg}</p>
    <p class="muted">
      Launch via <span class="mono">hc-spin</span> (it injects the conductor
      URL + token), or set <span class="mono">VITE_HC_ADMIN_PORT</span> for a
      packaged conductor.
    </p>
    <div><button class="primary" onclick={connect}>Retry</button></div>
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
