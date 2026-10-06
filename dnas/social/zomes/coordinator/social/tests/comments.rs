use holochain::prelude::*;
use holochain::sweettest::*;
use social::comment::*;

mod common;
use common::*;

#[tokio::test(flavor = "multi_thread")]
async fn comment_create_and_delete() {
    let (conductors, cells) = setup(2).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let target = fake_target(10);

    let created: Record = ca
        .call(
            &za,
            "create_comment",
            CreateCommentInput {
                target: target.clone(),
                target_role: "nft.2".into(),
                body: "lovely piece".into(),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let children: Vec<Record> = cb.call(&zb, "comments_for", target.clone()).await;
    assert_eq!(children.len(), 1);

    let mine: Vec<Record> = ca.call(&za, "my_comments", ()).await;
    assert_eq!(mine.len(), 1);

    let comment_hash = created.action_address().clone();
    let _: () = ca.call(&za, "delete_comment", comment_hash).await;
    await_consistency(&cells).await.unwrap();

    // The TargetToComment link is permanent, but get() on a deleted entry
    // resolves to nothing app-decodable — comments_for still returns the
    // link's target hash resolution as a None, filtered out by the batch get.
    let children_after: Vec<Record> = cb.call(&zb, "comments_for", target).await;
    assert!(
        children_after.is_empty(),
        "a deleted comment must not resolve"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn only_author_can_delete_comment() {
    let (conductors, cells) = setup(2).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let target = fake_target(11);

    let created: Record = ca
        .call(
            &za,
            "create_comment",
            CreateCommentInput {
                target,
                target_role: "nft.2".into(),
                body: "hi".into(),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let comment_hash = created.action_address().clone();
    let by_bob: Result<(), _> = cb.call_fallible(&zb, "delete_comment", comment_hash).await;
    assert!(by_bob.is_err(), "only the author may delete their comment");
}

#[tokio::test(flavor = "multi_thread")]
async fn comments_for_lists_only_direct_children() {
    let (conductors, cells) = setup(1).await;
    let z = cells[0].zome(ZOME);
    let c = conductors.get(0).unwrap();
    let target = fake_target(12);

    let top: Record = c
        .call(
            &z,
            "create_comment",
            CreateCommentInput {
                target: target.clone(),
                target_role: "nft.2".into(),
                body: "top level".into(),
            },
        )
        .await;
    let top_hash: AnyLinkableHash = top.action_address().clone().into();
    let _: Record = c
        .call(
            &z,
            "create_comment",
            CreateCommentInput {
                target: top_hash.clone(),
                target_role: "".into(),
                body: "a reply".into(),
            },
        )
        .await;

    let direct: Vec<Record> = c.call(&z, "comments_for", target).await;
    assert_eq!(direct.len(), 1, "only the top-level comment, not the reply");

    let replies: Vec<Record> = c.call(&z, "comments_for", top_hash).await;
    assert_eq!(
        replies.len(),
        1,
        "the reply is a direct child of the comment it replies to"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reply_target_can_be_a_comment() {
    let (conductors, cells) = setup(1).await;
    let z = cells[0].zome(ZOME);
    let c = conductors.get(0).unwrap();
    let target = fake_target(13);

    let top: Record = c
        .call(
            &z,
            "create_comment",
            CreateCommentInput {
                target,
                target_role: "nft.2".into(),
                body: "top".into(),
            },
        )
        .await;
    let top_hash: AnyLinkableHash = top.action_address().clone().into();

    let reply: Result<Record, _> = c
        .call_fallible(
            &z,
            "create_comment",
            CreateCommentInput {
                target: top_hash,
                target_role: "".into(),
                body: "re".into(),
            },
        )
        .await;
    assert!(
        reply.is_ok(),
        "a Comment's own action hash is a valid target"
    );
}
