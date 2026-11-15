#[path = "support/fake_hc.rs"]
mod fake_hc;

use std::collections::HashSet;

use fake_hc::FakeHcServer;
use herdr_tui::hc::rpc::RpcClient;
use herdr_tui::layout::presence::{PresenceDoc, PresenceLease, PresenceManager};
use herdr_tui::layout::store::LayoutStore;
use herdr_tui::layout::tree::{SplitAxis, SplitTree, TermRef};
use tempfile::TempDir;

#[tokio::test]
async fn store_and_presence() {
    let srv = FakeHcServer::start().await.expect("start fake server");
    let rpc = RpcClient::connect(srv.socket_path()).await.expect("rpc");
    let tmp = TempDir::new().unwrap();
    let fallback = tmp.path().join("layout.json");

    // 1. Doc API save & load (LY-3)
    let store = LayoutStore::new(fallback.clone(), &rpc.hello().methods);
    assert!(store.uses_doc_api());

    let (mut tree, p1) = SplitTree::with_single(Some(TermRef {
        address: "t1".into(),
    }));
    tree.split(
        p1,
        SplitAxis::Horizontal,
        Some(TermRef {
            address: "t2".into(),
        }),
    );

    let ver = store
        .save_space(Some(&rpc), "api", &tree)
        .await
        .expect("save api");
    assert_eq!(ver, 1);

    let (loaded, lver) = store.load(Some(&rpc)).await.expect("load");
    assert_eq!(lver, 1);
    assert_eq!(loaded.spaces.get("api"), Some(&tree));

    // Remove space via Doc API
    let ver_after_rm = store
        .remove_space(Some(&rpc), "api")
        .await
        .expect("remove api");
    assert_eq!(ver_after_rm, 2);
    let (loaded_after_rm, _) = store.load(Some(&rpc)).await.expect("load after rm");
    assert!(!loaded_after_rm.spaces.contains_key("api"));

    // 2. Fallback to layout.json when doc methods absent (LY-4)
    let store_no_doc =
        LayoutStore::new(fallback.clone(), &["hello".into(), "terminal.list".into()]);
    assert!(!store_no_doc.uses_doc_api());
    store_no_doc
        .save_space(None, "web", &tree)
        .await
        .expect("save local");
    let (local_doc, _) = store_no_doc.load(None).await.expect("load local");
    assert_eq!(local_doc.spaces.get("web"), Some(&tree));
    store_no_doc
        .remove_space(None, "web")
        .await
        .expect("remove local");
    let (local_after_rm, _) = store_no_doc.load(None).await.expect("load local after rm");
    assert!(!local_after_rm.spaces.contains_key("web"));

    // 3. Presence lease & foreign size checks (LY-5, LY-6)
    let pm = PresenceManager::new("inst-a".into());
    let mut pdoc = PresenceDoc::default();
    pdoc.leases.insert(
        "inst-b:t1".into(),
        PresenceLease {
            instance_id: "inst-b".into(),
            terminal: "t1".into(),
            updated_ms: 100_000,
        },
    );
    // Active foreign lease at 120_000 (20s old < 60s) -> blocked
    assert!(!pm.can_auto_promote(&pdoc, "t1", 120_000, None));
    // Stale foreign lease at 170_000 (70s old > 60s) -> allowed
    assert!(pm.can_auto_promote(&pdoc, "t1", 170_000, None));
    // Foreign size event within 10s -> blocked
    assert!(!pm.can_auto_promote(&pdoc, "t1", 170_000, Some(165_000)));
}

#[tokio::test]
async fn prune_empty_spaces_removes_empty_and_listed_spaces() {
    let srv = FakeHcServer::start().await.expect("start fake server");
    let rpc = RpcClient::connect(srv.socket_path()).await.expect("rpc");
    let tmp = TempDir::new().unwrap();
    let fallback = tmp.path().join("layout.json");
    let store = LayoutStore::new(fallback.clone(), &rpc.hello().methods);

    let (empty_single, _) = SplitTree::with_single(None);
    let (mut empty_multi, e1) = SplitTree::with_single(None);
    empty_multi.split(e1, SplitAxis::Horizontal, None);

    let (occupied, _) = SplitTree::with_single(Some(TermRef {
        address: "t1".into(),
    }));
    let (to_remove, _) = SplitTree::with_single(Some(TermRef {
        address: "t2".into(),
    }));

    store
        .save_space(Some(&rpc), "(unassigned)", &empty_multi)
        .await
        .unwrap();
    store
        .save_space(Some(&rpc), "empty-named", &empty_single)
        .await
        .unwrap();
    store
        .save_space(Some(&rpc), "keep-me", &occupied)
        .await
        .unwrap();
    store
        .save_space(Some(&rpc), "drop-me", &to_remove)
        .await
        .unwrap();

    let mut removed = HashSet::new();
    removed.insert("drop-me".to_string());

    store
        .prune_empty_spaces(Some(&rpc), &removed)
        .await
        .expect("prune doc api");

    let (doc, _) = store.load(Some(&rpc)).await.unwrap();
    assert!(!doc.spaces.contains_key("(unassigned)"));
    assert!(!doc.spaces.contains_key("empty-named"));
    assert!(!doc.spaces.contains_key("drop-me"));
    assert_eq!(doc.spaces.get("keep-me"), Some(&occupied));

    // Also verify local fallback without Doc API
    let local_fallback = tmp.path().join("local_only.json");
    let store_local = LayoutStore::new(local_fallback, &["hello".into()]);
    store_local
        .save_space(None, "(unassigned)", &empty_multi)
        .await
        .unwrap();
    store_local
        .save_space(None, "empty-named", &empty_single)
        .await
        .unwrap();
    store_local
        .save_space(None, "keep-me", &occupied)
        .await
        .unwrap();
    store_local
        .save_space(None, "drop-me", &to_remove)
        .await
        .unwrap();

    store_local
        .prune_empty_spaces(None, &removed)
        .await
        .expect("prune local");

    let (local_doc, _) = store_local.load(None).await.unwrap();
    assert!(!local_doc.spaces.contains_key("(unassigned)"));
    assert!(!local_doc.spaces.contains_key("empty-named"));
    assert!(!local_doc.spaces.contains_key("drop-me"));
    assert_eq!(local_doc.spaces.get("keep-me"), Some(&occupied));
}
