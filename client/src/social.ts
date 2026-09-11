import type {
  ActionHash,
  AgentPubKey,
  AnyDhtHash,
  AppWebsocket,
  Record as HcRecord,
} from "@holochain/client";
import { SignalType } from "@holochain/client";

/**
 * These mirror the Rust structs in `social_integrity` / `social`. `target` /
 * `AnyDhtHash` fields are Rust's `AnyLinkableHash` on the wire — usually
 * another cell's NFT action hash. `@holochain/client` doesn't distinguish
 * hash kinds at the type level (they're all `Uint8Array`), so `AnyDhtHash` is
 * the closest fit.
 */

/** The `Comment` entry as it decodes off the wire (`decodeEntry`). */
export interface CommentEntry {
  target: AnyDhtHash;
  target_role: string;
  body: string;
}

/** The `Collection` entry as it decodes off the wire (`decodeEntry`). */
export interface CollectionEntry {
  name: string;
  description: string;
}

export interface LikeTargetInput {
  target: AnyDhtHash;
  /** The `nft` clone's role name the target lives in (e.g. `"nft.3"`). */
  targetRole: string;
}

export interface LikesView {
  count: number;
  liked_by_me: boolean;
}

export interface MyLikedItem {
  target: AnyDhtHash;
  target_role: string;
}

export interface CreateCommentInput {
  target: AnyDhtHash;
  targetRole: string;
  body: string;
}

export interface CreateCollectionInput {
  name: string;
  description: string;
}

export interface UpdateCollectionInput {
  previousActionHash: ActionHash;
  name: string;
  description: string;
}

export interface AddToCollectionInput {
  collection: ActionHash;
  target: AnyDhtHash;
  targetRole: string;
}

export interface CollectionItemView {
  target: AnyDhtHash;
  target_role: string;
}

/** Best-effort handshake nudge delivered via `recv_remote_signal`. */
export type SocialRemoteSignal = { type: "Followed" };

export interface SocialClientOptions {
  client: AppWebsocket;
  /** Role name from `happ.yaml`. Default `"social"` — this is a single
   * global cell, never cloned, so there's normally no reason to override it. */
  roleName?: string;
  zomeName?: string;
}

export class SocialClient {
  private readonly client: AppWebsocket;
  private readonly roleName: string;
  private readonly zomeName: string;

  constructor(opts: SocialClientOptions) {
    this.client = opts.client;
    this.roleName = opts.roleName ?? "social";
    this.zomeName = opts.zomeName ?? "social";
  }

  private call<T>(fn_name: string, payload: unknown): Promise<T> {
    return this.client.callZome({
      role_name: this.roleName,
      zome_name: this.zomeName,
      fn_name,
      payload,
    }) as Promise<T>;
  }

  // --- follow --------------------------------------------------------------

  follow(agent: AgentPubKey): Promise<void> {
    return this.call("follow", agent);
  }

  unfollow(agent: AgentPubKey): Promise<void> {
    return this.call("unfollow", agent);
  }

  following(agent: AgentPubKey): Promise<AgentPubKey[]> {
    return this.call("following", agent);
  }

  followers(agent: AgentPubKey): Promise<AgentPubKey[]> {
    return this.call("followers", agent);
  }

  amIFollowing(agent: AgentPubKey): Promise<boolean> {
    return this.call("am_i_following", agent);
  }

  // --- like ------------------------------------------------------------

  like(args: LikeTargetInput): Promise<void> {
    return this.call("like", { target: args.target, target_role: args.targetRole });
  }

  unlike(target: AnyDhtHash): Promise<void> {
    return this.call("unlike", target);
  }

  likesFor(target: AnyDhtHash): Promise<LikesView> {
    return this.call("likes_for", target);
  }

  myLikes(): Promise<MyLikedItem[]> {
    return this.call("my_likes", null);
  }

  // --- comments --------------------------------------------------------

  createComment(args: CreateCommentInput): Promise<HcRecord> {
    return this.call("create_comment", {
      target: args.target,
      target_role: args.targetRole,
      body: args.body,
    });
  }

  deleteComment(comment: ActionHash): Promise<void> {
    return this.call("delete_comment", comment);
  }

  /** Direct children of `target` — top-level comments, or replies when
   * `target` is itself a comment's action hash. */
  commentsFor(target: AnyDhtHash): Promise<HcRecord[]> {
    return this.call("comments_for", target);
  }

  myComments(): Promise<HcRecord[]> {
    return this.call("my_comments", null);
  }

  // --- collections -------------------------------------------------------

  createCollection(args: CreateCollectionInput): Promise<HcRecord> {
    return this.call("create_collection", args);
  }

  /** Latest version (follows the edit chain). `null` if unknown. */
  getCollection(collection: ActionHash): Promise<HcRecord | null> {
    return this.call("get_collection", collection);
  }

  updateCollection(args: UpdateCollectionInput): Promise<HcRecord> {
    return this.call("update_collection", {
      previous_action_hash: args.previousActionHash,
      name: args.name,
      description: args.description,
    });
  }

  deleteCollection(collection: ActionHash): Promise<void> {
    return this.call("delete_collection", collection);
  }

  addToCollection(args: AddToCollectionInput): Promise<void> {
    return this.call("add_to_collection", {
      collection: args.collection,
      target: args.target,
      target_role: args.targetRole,
    });
  }

  removeFromCollection(collection: ActionHash, target: AnyDhtHash): Promise<void> {
    return this.call("remove_from_collection", { collection, target });
  }

  collectionItems(collection: ActionHash): Promise<CollectionItemView[]> {
    return this.call("collection_items", collection);
  }

  myCollections(): Promise<HcRecord[]> {
    return this.call("my_collections", null);
  }

  /** Every collection in the global directory, any curator. */
  allCollections(): Promise<HcRecord[]> {
    return this.call("all_collections", null);
  }

  // --- signals ---------------------------------------------------------

  onSignal(cb: (signal: SocialRemoteSignal) => void): () => void {
    return this.client.on("signal", (signal) => {
      if (signal.type !== SignalType.App) return;
      if (signal.value.zome_name !== this.zomeName) return;
      cb(signal.value.payload as SocialRemoteSignal);
    });
  }
}
