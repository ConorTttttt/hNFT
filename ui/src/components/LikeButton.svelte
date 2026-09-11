<script lang="ts">
  import type { AnyDhtHash } from "@holochain/client";
  import { useAppState } from "$lib/appState.svelte";

  let { target, targetRole }: { target: AnyDhtHash; targetRole: string } = $props();

  const app = useAppState();

  let count = $state(0);
  let likedByMe = $state(false);
  let loaded = $state(false);
  let busy = $state(false);

  $effect(() => {
    let cancelled = false;
    app.social
      .likesFor(target)
      .then((v) => {
        if (cancelled) return;
        count = v.count;
        likedByMe = v.liked_by_me;
        loaded = true;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  async function toggle(e: MouseEvent) {
    e.stopPropagation();
    busy = true;
    const was = likedByMe;
    // Optimistic flip — toggleLike() confirms with the real count on return.
    likedByMe = !was;
    count += was ? -1 : 1;
    try {
      const view = await app.toggleLike(target, targetRole, was);
      if (view) {
        count = view.count;
        likedByMe = view.liked_by_me;
      } else {
        // Failed — roll back.
        likedByMe = was;
        count += was ? 1 : -1;
      }
    } finally {
      busy = false;
    }
  }
</script>

<button class="ghost tiny" class:liked={likedByMe} onclick={toggle} disabled={busy || !loaded}>
  {likedByMe ? "♥" : "♡"} {count > 0 ? count : ""}
</button>

<style>
  .liked {
    color: var(--danger);
  }
</style>
