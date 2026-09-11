<script lang="ts">
  import type { ActionHash, AgentPubKey, AnyDhtHash } from "@holochain/client";
  import { decodeEntry, recordActionHash, type CommentEntry } from "@p2p-art-marketplace/client";
  import { useAppState } from "$lib/appState.svelte";
  import { b64, relativeTime, tsToMs } from "$lib/format";
  import AgentBadge from "./AgentBadge.svelte";

  let { target, targetRole }: { target: AnyDhtHash; targetRole: string } = $props();

  const app = useAppState();

  interface CommentRow {
    hash: ActionHash;
    author: AgentPubKey;
    body: string;
    atMs: number;
  }

  let comments = $state<CommentRow[]>([]);
  let loading = $state(true);
  let draft = $state("");
  let posting = $state(false);

  async function load() {
    loading = true;
    try {
      const records = await app.social.commentsFor(target);
      comments = records
        .map((r) => {
          const entry = decodeEntry<CommentEntry>(r);
          return {
            hash: recordActionHash(r),
            author: r.signed_action.hashed.content.header.author,
            body: entry.body,
            atMs: tsToMs(r.signed_action.hashed.content.header.timestamp),
          } satisfies CommentRow;
        })
        .sort((a, b) => a.atMs - b.atMs);
    } catch {
      comments = [];
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void target; // re-run when the target changes
    void load();
  });

  async function submit() {
    const body = draft.trim();
    if (!body) return;
    posting = true;
    try {
      await app.postComment(target, targetRole, body);
      draft = "";
      await load();
    } catch {
      // toasted by postComment
    } finally {
      posting = false;
    }
  }

  async function remove(hash: CommentRow["hash"]) {
    try {
      await app.deleteComment(hash);
      await load();
    } catch {
      // toasted by deleteComment
    }
  }
</script>

<div class="stack tight">
  <h4>Comments {#if comments.length}<span class="faint">· {comments.length}</span>{/if}</h4>

  {#if loading}
    <p class="muted small">Loading…</p>
  {:else if !comments.length}
    <p class="muted small">No comments yet.</p>
  {:else}
    <div class="stack tight">
      {#each comments as c (b64(c.hash))}
        <div class="comment">
          <AgentBadge agent={c.author} size={16} />
          <p class="body">{c.body}</p>
          <div class="row between">
            <span class="faint small">{relativeTime(c.atMs)}</span>
            {#if app.isMe(c.author)}
              <button class="ghost tiny" onclick={() => remove(c.hash)}>Delete</button>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}

  <form class="row" style="gap: 0.5rem" onsubmit={(e) => (e.preventDefault(), submit())}>
    <input class="grow" bind:value={draft} placeholder="Add a comment…" disabled={posting} />
    <button class="primary tiny" type="submit" disabled={posting || !draft.trim()}>
      {posting ? "…" : "Post"}
    </button>
  </form>
</div>

<style>
  .comment {
    padding: 0.5rem 0.6rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-elev);
  }
  .comment .body {
    margin: 0.3rem 0;
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>
