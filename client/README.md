# @p2p-art-marketplace/client

TypeScript client for the NFT hApp. Thin typed wrappers over `@holochain/client`
zome calls, plus the commit-reveal hashing the UI needs to mint and later reveal.

```
npm install
npm run typecheck   # tsc --noEmit
npm run build       # -> dist/
```

## Connect

```ts
import { AppWebsocket } from "@holochain/client";
import { NftClient } from "@p2p-art-marketplace/client";

const app = await AppWebsocket.connect();           // dev sandbox
const nft = new NftClient({ client: app });         // role "nft", zome "nft"
// per-collection clone cell:  new NftClient({ client: app, roleName: "nft.0" })
```

## Mint (blind) then reveal

`metadata_hash` is computed client-side; the salt + metadata stay off-chain until
you reveal. The hashing here reproduces `nft_integrity::reveal_preimage` +
`commit_hash` byte-for-byte (length-prefixed fields, then BLAKE2b-256).

```ts
import { prepareCommitment, recordActionHash } from "@p2p-art-marketplace/client";

const commitment = prepareCommitment(
  [{ trait_type: "background", value: "cobalt" }],
  "ipfs://bafy...",
);
// keep `commitment` (esp. commitment.salt) — you need it to reveal

const minted = await nft.mintFromCommitment(commitment);
// minted.nft is the NFT's permanent identity (mint action hash)

// later, by the minter only, once:
await nft.revealFromCommitment(minted.nft, minted.nft /* prev = mint */, commitment);
```

Reveal validation recomputes the hash; a wrong salt/attributes/uri is rejected.

## Transfer (two-phase handshake, no countersigning)

```ts
// seller
const commitment = await nft.initiateTransfer(nftHash, buyerPubKey);
// buyer (within transfer_window_secs)
await nft.acceptTransfer(commitment);
// seller, after being notified, tidy the stale index link
await nft.dropStaleOwnershipLink(nftHash);

// if the buyer never accepts, after the window:
await nft.cancelTransfer(commitment);   // then the NFT can be re-offered
```

## Read

```ts
await nft.currentOwner(nftHash);              // authoritative (provenance chain)
await nft.getNftsForAgent(agentPubKey);       // wallet view (derived index)
await nft.getProvenance(nftHash);             // ProvenanceHop[], ordered by sequence
const rec = await nft.getNft(nftHash);        // latest manifest version
```

```ts
import { decodeNftManifest } from "@p2p-art-marketplace/client";
if (rec) {
  const m = decodeNftManifest(rec);
  m.payload.status === "sealed" | "revealed";
}
```

## Signals

```ts
const off = nft.onSignal((s) => {
  if (s.type === "TransferOffered")  { /* refresh inbox */ }
  if (s.type === "TransferAccepted") { /* seller: drop stale link */ }
  // local post-commit Signals (EntryCreated / LinkCreated / …) also arrive here
});
// off() to unsubscribe
```

## Notes

- Hashes (`ActionHash`, `AgentPubKey`) are `Uint8Array`, as `@holochain/client` gives them.
- `[u8; 32]` fields (`metadata_hash`, `salt`) are sent as a MessagePack **array of
  ints**, not a `bin` — the wrappers convert; don't pass raw `Uint8Array` to `callZome`
  for those.
- Older Node without global Web Crypto: `import { webcrypto } from "node:crypto";
  globalThis.crypto = webcrypto as unknown as Crypto;` before `randomSalt()`.
