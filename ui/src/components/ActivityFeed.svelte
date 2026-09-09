<script lang="ts">
  import { useAppState } from "$lib/appState.svelte";
  import { relativeTime } from "$lib/format";
  import OfferInbox from "./OfferInbox.svelte";

  const app = useAppState();

  // tick so relative timestamps stay fresh
  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(id);
  });
</script>

<div class="stack" style="gap: 1.25rem">
  <div class="panel">
    <OfferInbox />
  </div>

  <div class="panel stack">
    <h3>Recent activity</h3>
    {#if !app.activity.length}
      <div class="empty small">
        Nothing yet this session. Mints, transfers and reveals show up here.
      </div>
    {:else}
      <div>
        {#each app.activity as item (item.id)}
          {@const rel = now >= 0 ? relativeTime(item.at) : ""}
          <div class="activity-item {item.kind}">
            <span class="bullet"></span>
            <div class="stack tight grow">
              <span>{item.text}</span>
              <span class="faint small">{rel}</span>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
