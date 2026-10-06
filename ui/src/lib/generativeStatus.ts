/**
 * Pure derivation of a generative NFT's UI status from on-chain facts. See
 * `docs/generative-collections.md` for the underlying protocol
 * (`GenerationRandomness` -> `GenerationRecord` -> `reveal_generated`).
 *
 * `traits-generated` and `ready-to-reveal` are ONE on-chain condition: creating
 * a `GenerationRecord` derives the seed and computes the traits in the same
 * call, so nothing on-chain distinguishes "traits computed" from "ready to
 * reveal" — a record either exists or it doesn't. They are kept as separate
 * enum members anyway (per product decision) because a future randomness
 * provider may introduce real latency between recording randomness and being
 * able to derive a record from it; `deriveGenerativeStatus` can only report
 * `ready-to-reveal` from chain data, and the UI is responsible for showing a
 * transient `traits-generated` moment right after a successful
 * `create_generation_record` call, before the next reload folds it into
 * `ready-to-reveal`.
 */
export type GenerativeStatus =
  | "unrevealed"
  | "randomness-pending"
  | "randomness-verified"
  | "traits-generated"
  | "ready-to-reveal"
  | "revealed";

export interface GenerativeStatusInputs {
  manifestStatus: "sealed" | "revealed";
  hasRandomness: boolean;
  hasRecord: boolean;
  /** A `create_generation_randomness`/`create_generation_record` call is in flight. */
  actionPending?: boolean;
}

export function deriveGenerativeStatus(input: GenerativeStatusInputs): GenerativeStatus {
  if (input.manifestStatus === "revealed") return "revealed";
  if (input.hasRecord) return "ready-to-reveal";
  if (input.actionPending) return "randomness-pending";
  if (input.hasRandomness) return "randomness-verified";
  return "unrevealed";
}

export const GENERATIVE_STATUS_LABEL: Record<GenerativeStatus, string> = {
  unrevealed: "Unrevealed",
  "randomness-pending": "Randomness pending…",
  "randomness-verified": "Randomness verified",
  "traits-generated": "Traits generated",
  "ready-to-reveal": "Ready to reveal",
  revealed: "Revealed",
};

/** Whether `reveal_generated` may be called for this status. */
export function canReveal(status: GenerativeStatus): boolean {
  return status === "ready-to-reveal" || status === "traits-generated";
}
