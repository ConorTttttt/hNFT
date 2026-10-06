use holochain::prelude::*;
use holochain::sweettest::*;
use social::collection::*;
use social_integrity::Collection;

mod common;
use common::*;

#[tokio::test(flavor = "multi_thread")]
async fn create_update_delete_collection_author_only() {
    let (conductors, cells) = setup(2).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());

    let created: Record = ca
        .call(
            &za,
            "create_collection",
            CreateCollectionInput {
                name: "Favorites".into(),
                description: "".into(),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    let hash = created.action_address().clone();

    // Bob cannot edit Alice's collection.
    let bad_update: Result<Record, _> = cb
        .call_fallible(
            &zb,
            "update_collection",
            UpdateCollectionInput {
                previous_action_hash: hash.clone(),
                name: "Hijacked".into(),
                description: "".into(),
            },
        )
        .await;
    assert!(
        bad_update.is_err(),
        "only the curator may edit their collection"
    );

    // Alice can.
    let updated: Record = ca
        .call(
            &za,
            "update_collection",
            UpdateCollectionInput {
                previous_action_hash: hash.clone(),
                name: "Top Picks".into(),
                description: "curated".into(),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let latest: Option<Record> = cb.call(&zb, "get_collection", hash.clone()).await;
    let entry: Collection = latest.expect("resolves").try_into().expect("decodes");
    assert_eq!(entry.name, "Top Picks");
    let _ = updated;

    // Bob cannot delete it either.
    let bad_delete: Result<(), _> = cb
        .call_fallible(&zb, "delete_collection", hash.clone())
        .await;
    assert!(
        bad_delete.is_err(),
        "only the curator may delete their collection"
    );

    let _: () = ca.call(&za, "delete_collection", hash).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn add_and_remove_item_curator_only() {
    let (conductors, cells) = setup(2).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let item = fake_target(30);

    let created: Record = ca
        .call(
            &za,
            "create_collection",
            CreateCollectionInput {
                name: "Wall".into(),
                description: "".into(),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    let collection = created.action_address().clone();

    // Bob cannot add to Alice's collection.
    let bad_add: Result<(), _> = cb
        .call_fallible(
            &zb,
            "add_to_collection",
            AddToCollectionInput {
                collection: collection.clone(),
                target: item.clone(),
                target_role: "nft.1".into(),
            },
        )
        .await;
    assert!(bad_add.is_err(), "only the curator may add items");

    let _: () = ca
        .call(
            &za,
            "add_to_collection",
            AddToCollectionInput {
                collection: collection.clone(),
                target: item.clone(),
                target_role: "nft.1".into(),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let items: Vec<CollectionItemView> = cb.call(&zb, "collection_items", collection.clone()).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].target_role, "nft.1");

    // Bob cannot remove it either.
    let bad_remove: Result<(), _> = cb
        .call_fallible(
            &zb,
            "remove_from_collection",
            RemoveFromCollectionInput {
                collection: collection.clone(),
                target: item.clone(),
            },
        )
        .await;
    assert!(bad_remove.is_err(), "only the curator may remove items");

    let _: () = ca
        .call(
            &za,
            "remove_from_collection",
            RemoveFromCollectionInput {
                collection: collection.clone(),
                target: item,
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    let items_after: Vec<CollectionItemView> = cb.call(&zb, "collection_items", collection).await;
    assert!(items_after.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn all_collections_visible_to_third_party() {
    let (conductors, cells) = setup(3).await;
    let (za, zb, zc) = (
        cells[0].zome(ZOME),
        cells[1].zome(ZOME),
        cells[2].zome(ZOME),
    );
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());

    let a: Record = ca
        .call(
            &za,
            "create_collection",
            CreateCollectionInput {
                name: "A's list".into(),
                description: "".into(),
            },
        )
        .await;
    let b: Record = cb
        .call(
            &zb,
            "create_collection",
            CreateCollectionInput {
                name: "B's list".into(),
                description: "".into(),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let all: Vec<Record> = conductors
        .get(2)
        .unwrap()
        .call(&zc, "all_collections", ())
        .await;
    let hashes: Vec<ActionHash> = all.iter().map(|r| r.action_address().clone()).collect();
    assert!(hashes.contains(a.action_address()));
    assert!(hashes.contains(b.action_address()));
}
