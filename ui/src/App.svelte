<script lang="ts">
  import { onMount } from "svelte";
  import { AppState, setCurrentAppState } from "$lib/appState.svelte";
  import { connectHolochain } from "$lib/holochain";
  import { b64 } from "$lib/format";
  import { toast } from "$lib/toast.svelte";
  import MintForm from "./components/MintForm.svelte";
  import NftDetail from "./components/NftDetail.svelte";
  import OfferInbox from "./components/OfferInbox.svelte";
  import Toaster from "./components/Toaster.svelte";
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
      phase = "ready";
      s.init();
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

  async function copyKey(key: Uint8Array) {
    try {
      await navigator.clipboard.writeText(b64(key));
      toast.info("Agent key copied");
    } catch {
      toast.error("Clipboard unavailable");
    }
  }
</script>

<Toaster />

{#if phase === "connecting"}
  <main class="center"><p class="muted">Connecting to the conductor…</p></main>
{:else if phase === "error"}
  <main class="center stack" style="max-width: 460px">
    <h3>Can't reach the conductor</h3>
    <p class="error small mono">{errMsg}</p>
    <p class="muted small">
      Launch via <span class="mono">hc-spin</span> (it injects the conductor URL +
      token), or set <span class="mono">VITE_HC_ADMIN_PORT</span> for a packaged
      conductor.
    </p>
    <div><button class="primary" onclick={connect}>Retry</button></div>
  </main>
{:else if app}
  {@const me = app.me}
  <header>
    <div class="row" style="gap: 0.6rem">
      <span class="dot"></span>
      <strong>{app.collection?.name ?? "Collection"}</strong>
      {#if app.collection}
        <span class="pill">{app.collection.symbol}</span>
      {/if}
      {#if app.isMinter}<span class="pill accent">minter</span>{/if}
    </div>
    <button
      class="chip"
      title="click to copy your agent key"
      onclick={() => copyKey(me)}
    >
      <span class="mono truncate" style="max-width: 220px">{b64(app.me)}</span>
      <span class="faint">⧉</span>
    </button>
  </header>

  <main class="workspace">
    <section class="stack">
      <div class="row">
        <button class:primary={tab === "wallet"} onclick={() => (tab = "wallet")}
          >Wallet</button
        >
        <button class:primary={tab === "mint"} onclick={() => (tab = "mint")}
          >Mint</button
        >
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

<style>
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    padding: 0.75rem 1.25rem;
    border-bottom: 1px solid var(--border);
    background: color-mix(in oklab, var(--bg-elev) 85%, transparent);
    position: sticky;
    top: 0;
    backdrop-filter: blur(8px);
    z-index: 10;
  }
  .workspace {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 340px;
    gap: 1.25rem;
    padding: 1.25rem;
    align-items: start;
    max-width: 1200px;
    margin: 0 auto;
  }
  .center {
    min-height: 70vh;
    display: grid;
    place-content: center;
    padding: 2rem;
    text-align: center;
  }
  @media (max-width: 820px) {
    .workspace {
      grid-template-columns: 1fr;
    }
  }
</style>
