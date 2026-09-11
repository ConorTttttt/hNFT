use holochain::prelude::*;
use holochain::sweettest::*;
use social::like::*;

mod common;
use common::*;

#[tokio::test(flavor = "multi_thread")]
async fn follow_and_unfollow_updates_both_indexes() {
    let (conductors, cells) = setup(2).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let (ka, kb) = (cells[0].agent_pubkey().clone(), cells[1].agent_pubkey().clone());

    let _: () = ca.call(&za, "follow", kb.clone()).await;
    await_consistency(&cells).await.unwrap();

    let a_following: Vec<AgentPubKey> = ca.call(&za, "following", ka.clone()).await;
    assert_eq!(a_following, vec![kb.clone()]);
    let b_followers: Vec<AgentPubKey> = cb.call(&zb, "followers", kb.clone()).await;
    assert_eq!(b_followers, vec![ka.clone()]);
    let am_following: bool = ca.call(&za, "am_i_following", kb.clone()).await;
    assert!(am_following);

    let _: () = ca.call(&za, "unfollow", kb.clone()).await;
    await_consistency(&cells).await.unwrap();

    let a_following_after: Vec<AgentPubKey> = ca.call(&za, "following", ka.clone()).await;
    assert!(a_following_after.is_empty());
    let b_followers_after: Vec<AgentPubKey> = cb.call(&zb, "followers", kb).await;
    assert!(b_followers_after.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn cannot_follow_self() {
    let (conductors, cells) = setup(1).await;
    let z = cells[0].zome(ZOME);
    let c = conductors.get(0).unwrap();
    let me = cells[0].agent_pubkey().clone();

    let result: Result<(), _> = c.call_fallible(&z, "follow", me).await;
    assert!(result.is_err(), "following yourself must be rejected");
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_follower_can_unfollow() {
    let (conductors, cells) = setup(2).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let (ka, kb) = (cells[0].agent_pubkey().clone(), cells[1].agent_pubkey().clone());

    let _: () = ca.call(&za, "follow", kb.clone()).await;
    await_consistency(&cells).await.unwrap();

    // Bob calling "unfollow" with Alice's key deletes nothing of Bob's own —
    // it's a no-op for Bob (he never authored that edge) rather than an
    // error, but the edge must survive regardless.
    let _: () = cb.call(&zb, "unfollow", ka.clone()).await;
    await_consistency(&cells).await.unwrap();

    let a_following: Vec<AgentPubKey> = ca.call(&za, "following", ka).await;
    assert_eq!(a_following, vec![kb], "only Alice's own unfollow may remove her edge");
}

#[tokio::test(flavor = "multi_thread")]
async fn like_is_idempotent_and_reversible() {
    let (conductors, cells) = setup(2).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let target = fake_target(1);

    let input = || LikeTargetInput {
        target: target.clone(),
        target_role: "nft.0".into(),
    };
    let _: () = ca.call(&za, "like", input()).await;
    let _: () = ca.call(&za, "like", input()).await; // idempotent, no error
    await_consistency(&cells).await.unwrap();

    let view: LikesView = cb.call(&zb, "likes_for", target.clone()).await;
    assert_eq!(view.count, 1, "double-liking must not double the count");
    assert!(!view.liked_by_me, "Bob hasn't liked it");

    let mine: Vec<MyLikedItem> = ca.call(&za, "my_likes", ()).await;
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].target_role, "nft.0");

    let _: () = ca.call(&za, "unlike", target.clone()).await;
    await_consistency(&cells).await.unwrap();
    let view_after: LikesView = cb.call(&zb, "likes_for", target).await;
    assert_eq!(view_after.count, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn likes_for_counts_and_reflects_me() {
    let (conductors, cells) = setup(2).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let target = fake_target(2);

    let _: () = ca
        .call(
            &za,
            "like",
            LikeTargetInput { target: target.clone(), target_role: "nft.0".into() },
        )
        .await;
    let _: () = cb
        .call(
            &zb,
            "like",
            LikeTargetInput { target: target.clone(), target_role: "nft.0".into() },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let from_a: LikesView = ca.call(&za, "likes_for", target.clone()).await;
    assert_eq!(from_a.count, 2);
    assert!(from_a.liked_by_me);
    let from_b: LikesView = cb.call(&zb, "likes_for", target).await;
    assert_eq!(from_b.count, 2);
    assert!(from_b.liked_by_me);
}
