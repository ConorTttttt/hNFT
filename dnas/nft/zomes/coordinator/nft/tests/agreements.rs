//! Reveal-agreement flows (phase 1): the additive `MintAgreement` path, the
//! delegated reveal, the deadline-gated `RevealDefault`, and the parked private
//! `RevealSecret`. The plain commit-reveal path is covered by `flows.rs` /
//! `edge.rs` and must be unaffected — `plain_mint_unchanged` guards that here.

use holochain::prelude::*;
use holochain::sweettest::*;
use nft::agreement::{AgreementView, DelegatedRevealView};
use nft::mint::{MintNftInput, MintNftOutput, MintWithAgreementInput, RevealSecretInput};
use nft::reveal::RevealNftInput;
use nft_integrity::*;

mod common;
use common::*;

/// `(salt, attributes, image_uri, commitment)` for a deterministic reveal.
fn preimage(seed: u8) -> ([u8; 32], Vec<NftAttribute>, String, [u8; 32]) {
    let salt = [seed; 32];
    let attributes = vec![NftAttribute {
        trait_type: "edition".into(),
        value: format!("{seed}"),
    }];
    let image_uri = format!("ipfs://art/{seed}");
    let commitment = commit_hash(&reveal_preimage(&salt, &attributes, &image_uri));
    (salt, attributes, image_uri, commitment)
}

const MICROS_PER_SEC: i64 = 1_000_000;

fn now_micros() -> i64 {
    Timestamp::now().as_micros()
}

/// `mint_with_agreement` writes the agreement + `NftToAgreement` link, and
/// `agreement_for` reflects it.
#[tokio::test(flavor = "multi_thread")]
async fn mint_with_agreement_creates_it() {
    let (conductors, cells) = setup(2, None).await;
    let alice = cells[0].zome(ZOME);
    let ca = conductors.get(0).unwrap();
    let delegate = cells[1].agent_pubkey().clone();

    let (_, _, _, commitment) = preimage(1);
    let reveal_by = now_micros() + 60 * MICROS_PER_SEC;
    let default_after = reveal_by + 3600 * MICROS_PER_SEC;

    let minted: MintNftOutput = ca
        .call(
            &alice,
            "mint_with_agreement",
            MintWithAgreementInput {
                metadata_hash: commitment,
                reveal_by_micros: reveal_by,
                default_after_micros: default_after,
                delegate: Some(delegate.clone()),
                secret: None,
            },
        )
        .await;
    assert!(minted.agreement.is_some(), "output carries the agreement hash");

    let view: Option<AgreementView> = ca
        .call(&alice, "agreement_for", minted.nft.clone())
        .await;
    let view = view.expect("agreement_for returns the agreement");
    assert_eq!(view.reveal_by_micros, reveal_by);
    assert_eq!(view.default_after_micros, default_after);
    assert_eq!(view.delegate.as_ref(), Some(&delegate));
    assert!(!view.defaulted, "not defaulted at mint");

    // The delegate sees it in their delegated-reveals list.
    await_consistency(&cells).await.unwrap();
    let bob = cells[1].zome(ZOME);
    let cb = conductors.get(1).unwrap();
    let mine: Vec<DelegatedRevealView> = cb.call(&bob, "my_delegated_reveals", ()).await;
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].nft, minted.nft);
    assert!(mine[0].sealed);
}

/// A named delegate may reveal — but only once `reveal_by` has passed.
#[tokio::test(flavor = "multi_thread")]
async fn delegate_reveals_after_reveal_by() {
    let (conductors, cells) = setup(2, None).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let delegate = cells[1].agent_pubkey().clone();

    let (salt, attributes, image_uri, commitment) = preimage(2);
    // Generous offset: it must still be in the future after `await_consistency`
    // has propagated the agreement to the delegate's node.
    let reveal_by = now_micros() + 30 * MICROS_PER_SEC;
    let default_after = reveal_by + 3600 * MICROS_PER_SEC;

    let minted: MintNftOutput = ca
        .call(
            &za,
            "mint_with_agreement",
            MintWithAgreementInput {
                metadata_hash: commitment,
                reveal_by_micros: reveal_by,
                default_after_micros: default_after,
                delegate: Some(delegate),
                secret: None,
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let reveal_input = || RevealNftInput {
        nft: minted.nft.clone(),
        previous_action_hash: minted.nft.clone(),
        salt,
        attributes: attributes.clone(),
        image_uri: image_uri.clone(),
        force: false,
    };

    // The delegate can see the agreement, so a pre-`reveal_by` rejection is the
    // time gate talking, not missing data.
    let view: Option<AgreementView> = cb.call(&zb, "agreement_for", minted.nft.clone()).await;
    assert!(view.is_some(), "delegate can see the agreement");

    // Before reveal_by: rejected.
    let early: Result<Record, _> = cb.call_fallible(&zb, "reveal_nft", reveal_input()).await;
    assert!(early.is_err(), "delegate cannot reveal before reveal_by");

    // After reveal_by: accepted.
    tokio::time::sleep(std::time::Duration::from_secs(35)).await;
    let ok: Record = cb.call(&zb, "reveal_nft", reveal_input()).await;
    let manifest = NftManifest::try_from(ok).unwrap();
    assert!(matches!(manifest.payload, NftPayload::Revealed { .. }));
}

/// An agent who is neither the minter nor the delegate is rejected, agreement
/// or not.
#[tokio::test(flavor = "multi_thread")]
async fn third_party_cannot_reveal() {
    let (conductors, cells) = setup(2, None).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());

    let (salt, attributes, image_uri, commitment) = preimage(3);
    let reveal_by = now_micros() + 2 * MICROS_PER_SEC;
    let default_after = reveal_by + 3600 * MICROS_PER_SEC;

    let minted: MintNftOutput = ca
        .call(
            &za,
            "mint_with_agreement",
            MintWithAgreementInput {
                metadata_hash: commitment,
                reveal_by_micros: reveal_by,
                default_after_micros: default_after,
                delegate: None, // no delegate at all
                secret: None,
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_secs(4)).await;

    let by_bob: Result<Record, _> = cb
        .call_fallible(
            &zb,
            "reveal_nft",
            RevealNftInput {
                nft: minted.nft.clone(),
                previous_action_hash: minted.nft.clone(),
                salt,
                attributes,
                image_uri,
                force: false,
            },
        )
        .await;
    assert!(by_bob.is_err(), "a non-minter non-delegate cannot reveal");
}

/// `mark_reveal_default` is rejected before `default_after` and accepted after;
/// once defaulted, an un-forced reveal is refused and a forced one goes through.
#[tokio::test(flavor = "multi_thread")]
async fn reveal_default_gated_by_deadline() {
    // Single node: the marker and the minter are the same agent, so the
    // "before deadline" call lands locally and deterministically before the gate.
    let (conductors, cells) = setup(1, None).await;
    let za = cells[0].zome(ZOME);
    let ca = conductors.get(0).unwrap();

    let (salt, attributes, image_uri, commitment) = preimage(4);
    let reveal_by = now_micros() + 3 * MICROS_PER_SEC;
    let default_after = now_micros() + 6 * MICROS_PER_SEC;

    let minted: MintNftOutput = ca
        .call(
            &za,
            "mint_with_agreement",
            MintWithAgreementInput {
                metadata_hash: commitment,
                reveal_by_micros: reveal_by,
                default_after_micros: default_after,
                delegate: None,
                secret: None,
            },
        )
        .await;

    // Before default_after: rejected (called immediately, no network wait).
    let early: Result<Record, _> = ca
        .call_fallible(&za, "mark_reveal_default", minted.nft.clone())
        .await;
    assert!(early.is_err(), "cannot mark defaulted before default_after");

    // After default_after: accepted.
    tokio::time::sleep(std::time::Duration::from_secs(8)).await;
    let _: Record = ca.call(&za, "mark_reveal_default", minted.nft.clone()).await;

    let view: Option<AgreementView> = ca.call(&za, "agreement_for", minted.nft.clone()).await;
    assert!(view.expect("agreement").defaulted, "now flagged defaulted");

    // The minter's un-forced reveal is refused…
    let unforced: Result<Record, _> = ca
        .call_fallible(
            &za,
            "reveal_nft",
            RevealNftInput {
                nft: minted.nft.clone(),
                previous_action_hash: minted.nft.clone(),
                salt,
                attributes: attributes.clone(),
                image_uri: image_uri.clone(),
                force: false,
            },
        )
        .await;
    assert!(unforced.is_err(), "defaulted reveal needs force");

    // …but a forced (late) reveal still works.
    let forced: Record = ca
        .call(
            &za,
            "reveal_nft",
            RevealNftInput {
                nft: minted.nft.clone(),
                previous_action_hash: minted.nft.clone(),
                salt,
                attributes,
                image_uri,
                force: true,
            },
        )
        .await;
    let manifest = NftManifest::try_from(forced).unwrap();
    assert!(matches!(manifest.payload, NftPayload::Revealed { .. }));
}

/// The plain `mint_nft` → `reveal_nft` path is untouched by the agreement work.
#[tokio::test(flavor = "multi_thread")]
async fn plain_mint_unchanged() {
    let (conductors, cells) = setup(1, None).await;
    let alice = cells[0].zome(ZOME);
    let c = conductors.get(0).unwrap();

    let (salt, attributes, image_uri, commitment) = preimage(5);
    let minted: MintNftOutput = c
        .call(&alice, "mint_nft", MintNftInput { metadata_hash: commitment })
        .await;
    assert!(minted.agreement.is_none(), "no agreement on a plain mint");

    let none: Option<AgreementView> = c.call(&alice, "agreement_for", minted.nft.clone()).await;
    assert!(none.is_none(), "agreement_for is empty for a plain mint");

    let revealed: Record = c
        .call(
            &alice,
            "reveal_nft",
            RevealNftInput {
                nft: minted.nft.clone(),
                previous_action_hash: minted.nft.clone(),
                salt,
                attributes,
                image_uri,
                force: false,
            },
        )
        .await;
    let manifest = NftManifest::try_from(revealed).unwrap();
    match manifest.payload {
        NftPayload::Revealed { agreement, .. } => {
            assert!(agreement.is_none(), "no agreement reference on the reveal")
        }
        _ => panic!("expected Revealed"),
    }
}

/// A secret parked at mint is retrievable via `reveal_secret_for` and drives a
/// successful reveal. `start_auto_reveal` is callable (the live sweep is
/// exercised in the app, not here).
#[tokio::test(flavor = "multi_thread")]
async fn stored_secret_reveal_path() {
    let (conductors, cells) = setup(1, None).await;
    let alice = cells[0].zome(ZOME);
    let c = conductors.get(0).unwrap();

    let (salt, attributes, image_uri, commitment) = preimage(6);
    // Far-future reveal_by so the background sweep never fires during the test.
    let reveal_by = now_micros() + 3600 * MICROS_PER_SEC;
    let default_after = reveal_by + 3600 * MICROS_PER_SEC;

    let minted: MintNftOutput = c
        .call(
            &alice,
            "mint_with_agreement",
            MintWithAgreementInput {
                metadata_hash: commitment,
                reveal_by_micros: reveal_by,
                default_after_micros: default_after,
                delegate: None,
                secret: Some(RevealSecretInput {
                    salt,
                    attributes: attributes.clone(),
                    image_uri: image_uri.clone(),
                }),
            },
        )
        .await;

    let () = c.call(&alice, "start_auto_reveal", ()).await;

    let stored: Option<RevealSecretInput> = c
        .call(&alice, "reveal_secret_for", minted.nft.clone())
        .await;
    let stored = stored.expect("the parked secret is retrievable");
    assert_eq!(stored.salt, salt);
    assert_eq!(stored.attributes, attributes);
    assert_eq!(stored.image_uri, image_uri);

    // The minter may reveal at any time; feed the retrieved secret straight back.
    let revealed: Record = c
        .call(
            &alice,
            "reveal_nft",
            RevealNftInput {
                nft: minted.nft.clone(),
                previous_action_hash: minted.nft.clone(),
                salt: stored.salt,
                attributes: stored.attributes,
                image_uri: stored.image_uri,
                force: false,
            },
        )
        .await;
    let manifest = NftManifest::try_from(revealed).unwrap();
    match manifest.payload {
        NftPayload::Revealed { agreement, .. } => {
            assert!(agreement.is_some(), "reveal records the agreement it ran under")
        }
        _ => panic!("expected Revealed"),
    }
}
