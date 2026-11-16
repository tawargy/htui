#[path = "support/fake_hc.rs"]
mod fake_hc;

use std::collections::HashSet;

use fake_hc::FakeHcServer;
use herdr_tui::hc::rpc::RpcClient;
use herdr_tui::layout::panel::SpaceLayout;
use herdr_tui::layout::presence::{PresenceDoc, PresenceLease, PresenceManager};
use herdr_tui::layout::store::{LayoutDoc, LayoutStore, LAYOUT_DOC_NAME};
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
    let space_layout = SpaceLayout::from_tree(tree);

    let ver = store
        .save_space(Some(&rpc), "api", &space_layout)
        .await
        .expect("save api");
    assert_eq!(ver, 1);

    let (loaded, lver) = store.load(Some(&rpc)).await.expect("load");
    assert_eq!(lver, 1);
    assert_eq!(loaded.spaces.get("api"), Some(&space_layout));

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
        .save_space(None, "web", &space_layout)
        .await
        .expect("save local");
    let (local_doc, _) = store_no_doc.load(None).await.expect("load local");
    assert_eq!(local_doc.spaces.get("web"), Some(&space_layout));
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

    let (empty_single, _, _) = SpaceLayout::with_single(None);
    let (mut empty_multi_tree, e1) = SplitTree::with_single(None);
    empty_multi_tree.split(e1, SplitAxis::Horizontal, None);
    let empty_multi = SpaceLayout::from_tree(empty_multi_tree);

    let (occupied, _, _) = SpaceLayout::with_single(Some(TermRef {
        address: "t1".into(),
    }));
    let (to_remove, _, _) = SpaceLayout::with_single(Some(TermRef {
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

#[tokio::test]
async fn legacy_split_tree_migration_and_multipanel_roundtrip() {
    let srv = FakeHcServer::start().await.expect("start fake server");
    let rpc = RpcClient::connect(srv.socket_path()).await.expect("rpc");
    let tmp = TempDir::new().unwrap();
    let fallback = tmp.path().join("layout.json");
    let store = LayoutStore::new(fallback.clone(), &rpc.hello().methods);

    // Construct legacy SplitTree JSON with split nodes, custom ratio, and TermRefs
    let (mut legacy_tree, p1) = SplitTree::with_single(Some(TermRef {
        address: "t-old-1".into(),
    }));
    let _p2 = legacy_tree
        .split(
            p1,
            SplitAxis::Horizontal,
            Some(TermRef {
                address: "t-old-2".into(),
            }),
        )
        .unwrap();
    legacy_tree.set_ratio_at(p1, 0.35);

    let legacy_json = serde_json::json!({
        "spaces": {
            "api": legacy_tree
        }
    });

    // 1. Direct LayoutDoc deserialization migrates legacy SplitTree into 1-panel SpaceLayout
    let parsed: LayoutDoc = serde_json::from_value(legacy_json.clone()).expect("parse legacy");
    let api_space = parsed.spaces.get("api").expect("api space");
    assert_eq!(api_space.panels.len(), 1);
    assert_eq!(api_space.active_panel, Some(1));
    assert_eq!(api_space.panels[0].tree, legacy_tree);
    assert_eq!(api_space.panels[0].focused_pane, Some(p1));

    // 2. Doc API migration: store legacy JSON in htui.layout, load via LayoutStore, then add a 2nd panel and save
    rpc.doc_set(LAYOUT_DOC_NAME, legacy_json.clone(), 0, None)
        .await
        .expect("seed legacy doc");
    let (loaded_doc, ver) = store.load(Some(&rpc)).await.expect("load legacy doc");
    assert_eq!(ver, 1);
    let mut migrated_api = loaded_doc.spaces.get("api").expect("api").clone();
    assert_eq!(migrated_api.panels.len(), 1);
    assert_eq!(migrated_api.panels[0].tree, legacy_tree);

    let (panel2_id, _) = migrated_api.add_panel(Some(TermRef {
        address: "t-new-3".into(),
    }));
    let new_ver = store
        .save_space(Some(&rpc), "api", &migrated_api)
        .await
        .expect("save multipanel");
    assert_eq!(new_ver, 2);

    // Reloading from Doc API returns the persisted multi-panel SpaceLayout
    let (reloaded, _) = store.load(Some(&rpc)).await.expect("reload multipanel");
    let reloaded_api = reloaded.spaces.get("api").expect("api");
    assert_eq!(reloaded_api.panels.len(), 2);
    assert_eq!(reloaded_api.active_panel, Some(panel2_id));
    assert_eq!(reloaded_api.panels[0].tree, legacy_tree);
    assert_eq!(
        reloaded_api.panels[1].terminal_addresses(),
        vec!["t-new-3".to_string()]
    );

    // 3. Local layout.json fallback migration & multi-panel persistence
    let local_legacy_path = tmp.path().join("legacy_local.json");
    std::fs::write(
        &local_legacy_path,
        serde_json::to_string_pretty(&legacy_json).unwrap(),
    )
    .unwrap();
    let local_store = LayoutStore::new(local_legacy_path.clone(), &["hello".into()]);
    let (local_loaded, _) = local_store.load(None).await.expect("load local legacy");
    assert_eq!(local_loaded.spaces["api"].panels.len(), 1);
    assert_eq!(local_loaded.spaces["api"].panels[0].tree, legacy_tree);

    local_store
        .save_space(None, "api", &migrated_api)
        .await
        .expect("save local multipanel");
    let (local_reloaded, _) = local_store.load(None).await.expect("reload local");
    assert_eq!(local_reloaded.spaces["api"], migrated_api);
    let raw_saved = std::fs::read_to_string(&local_legacy_path).unwrap();
    assert!(raw_saved.contains("\"panels\""));
}

#[tokio::test]
async fn multipanel_persistence_under_cas_conflicts_and_per_space_merge() {
    let srv = FakeHcServer::start().await.expect("start fake server");
    let rpc1 = RpcClient::connect(srv.socket_path()).await.expect("rpc1");
    let rpc2 = RpcClient::connect(srv.socket_path()).await.expect("rpc2");
    let tmp = TempDir::new().unwrap();
    let store1 = LayoutStore::new(tmp.path().join("layout1.json"), &rpc1.hello().methods);
    let store2 = LayoutStore::new(tmp.path().join("layout2.json"), &rpc2.hello().methods);

    let (mut api_space, api_p1, api_pane1) = SpaceLayout::with_single(Some(TermRef {
        address: "t-api-1".into(),
    }));
    api_space
        .panel_mut(api_p1)
        .unwrap()
        .tree
        .split(
            api_pane1,
            SplitAxis::Horizontal,
            Some(TermRef {
                address: "t-api-2".into(),
            }),
        )
        .unwrap();
    api_space.add_panel(Some(TermRef {
        address: "t-api-3".into(),
    }));

    let (mut web_space, _, _) = SpaceLayout::with_single(Some(TermRef {
        address: "t-web-1".into(),
    }));
    web_space.add_panel(Some(TermRef {
        address: "t-web-2".into(),
    }));

    // Run concurrent CAS saves on separate spaces; retry loop merges both cleanly
    let (res1, res2) = tokio::join!(
        store1.save_space(Some(&rpc1), "api", &api_space),
        store2.save_space(Some(&rpc2), "web", &web_space),
    );
    res1.expect("api CAS save");
    res2.expect("web CAS save");

    let (doc, ver) = store1.load(Some(&rpc1)).await.expect("load merged");
    assert_eq!(ver, 2);
    assert_eq!(doc.spaces.get("api"), Some(&api_space));
    assert_eq!(doc.spaces.get("web"), Some(&web_space));
}
