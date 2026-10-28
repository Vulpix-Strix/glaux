use std::collections::BTreeMap;

use pretty_assertions::assert_eq;
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

use super::*;

fn docs() -> Documents {
    Documents::new(64)
}

#[test]
fn consecutive_changes_merge_into_the_latest_text() {
    let mut documents = docs();
    documents.open("a", "luau", 1, "one").unwrap();
    documents.drain();
    documents.change("a", 2, "two");
    documents.change("a", 3, "three");
    assert_eq!(
        documents.drain(),
        vec![Wire::Change {
            uri: "a".into(),
            version: 3,
            text: "three".into()
        }]
    );
}

#[test]
fn a_change_before_the_open_is_sent_goes_inside_the_open() {
    let mut documents = docs();
    documents.open("a", "luau", 1, "one").unwrap();
    documents.change("a", 2, "two");
    assert_eq!(
        documents.drain(),
        vec![Wire::Open {
            uri: "a".into(),
            language: "luau".into(),
            version: 2,
            text: "two".into()
        }]
    );
}

#[test]
fn open_and_close_are_both_kept_even_when_nothing_was_sent() {
    let mut documents = docs();
    documents.open("a", "luau", 1, "x").unwrap();
    documents.close("a").unwrap();
    let wire = documents.drain();
    assert!(matches!(wire[0], Wire::Open { .. }));
    assert_eq!(wire[1], Wire::Close { uri: "a".into() });
    assert_eq!(wire.len(), 2);
}

#[test]
fn a_change_pending_at_close_is_dropped_and_none_follows() {
    let mut documents = docs();
    documents.open("a", "luau", 1, "x").unwrap();
    documents.drain();
    documents.change("a", 2, "y");
    documents.close("a").unwrap();
    documents.change("a", 3, "late");
    assert_eq!(documents.drain(), vec![Wire::Close { uri: "a".into() }]);
}

#[test]
fn reopening_after_a_close_sends_close_then_open() {
    let mut documents = docs();
    documents.open("a", "luau", 1, "x").unwrap();
    documents.drain();
    documents.close("a").unwrap();
    documents.open("a", "luau", 7, "fresh").unwrap();
    let wire = documents.drain();
    assert_eq!(wire[0], Wire::Close { uri: "a".into() });
    assert!(matches!(&wire[1], Wire::Open { version: 7, text, .. } if text == "fresh"));
}

#[test]
fn unknown_documents_are_ignored() {
    let mut documents = docs();
    documents.change("ghost", 1, "x");
    documents.close("ghost").unwrap();
    assert_eq!(documents.drain(), vec![]);
}

#[test]
fn lifecycle_notifications_are_refused_when_full_but_changes_never_are() {
    let mut documents = Documents::new(2);
    documents.open("a", "luau", 1, "x").unwrap();
    documents.open("b", "luau", 1, "x").unwrap();
    assert_eq!(documents.open("c", "luau", 1, "x"), Err(QueueFull));
    assert_eq!(documents.close("a"), Err(QueueFull));
    documents.drain();
    for version in 2..1000 {
        documents.change("a", version, "y");
    }
    assert_eq!(documents.drain().len(), 1);
}

#[test]
fn resync_owes_every_open_document_with_its_latest_text() {
    let mut documents = docs();
    documents.open("a", "luau", 1, "x").unwrap();
    documents.open("b", "luau", 1, "x").unwrap();
    documents.drain();
    documents.change("a", 5, "latest");
    documents.close("b").unwrap();
    documents.resync();
    assert_eq!(
        documents.drain(),
        vec![Wire::Open {
            uri: "a".into(),
            language: "luau".into(),
            version: 5,
            text: "latest".into()
        }]
    );
}

#[derive(Debug, Clone)]
enum Step {
    Open(u8),
    Change(u8),
    Close(u8),
    Drain,
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    proptest::collection::vec(
        prop_oneof![
            (0..3u8).prop_map(Step::Open),
            (0..3u8).prop_map(Step::Change),
            (0..3u8).prop_map(Step::Close),
            Just(Step::Drain),
        ],
        0..80,
    )
}

proptest! {
    /// Whatever the client does, a server applying the drained stream sees a
    /// valid lifecycle per document and ends with the client's latest text.
    #[test]
    fn the_drained_stream_is_always_a_valid_lifecycle(steps in steps()) {
        let mut documents = Documents::new(10_000);
        let mut server: BTreeMap<String, (i64, String)> = BTreeMap::new();
        let mut version = 0;
        let apply = |documents: &mut Documents, server: &mut BTreeMap<String, (i64, String)>| -> Result<(), TestCaseError> {
            for wire in documents.drain() {
                match wire {
                    Wire::Open { uri, version, text, .. } => {
                        prop_assert!(server.insert(uri, (version, text)).is_none(), "open twice");
                    }
                    Wire::Change { uri, version, text } => {
                        prop_assert!(server.contains_key(&uri), "change outside lifecycle");
                        server.insert(uri, (version, text));
                    }
                    Wire::Close { uri } => {
                        prop_assert!(server.remove(&uri).is_some(), "close of unopened");
                    }
                }
            }
            Ok(())
        };
        for step in steps {
            version += 1;
            match step {
                Step::Open(n) => { documents.open(&format!("u{n}"), "luau", version, &format!("t{version}")).unwrap(); }
                Step::Change(n) => documents.change(&format!("u{n}"), version, &format!("t{version}")),
                Step::Close(n) => { documents.close(&format!("u{n}")).unwrap(); }
                Step::Drain => apply(&mut documents, &mut server)?,
            }
        }
        apply(&mut documents, &mut server)?;
        let expected: BTreeMap<String, (i64, String)> = documents
            .versions()
            .into_iter()
            .map(|(uri, version)| (uri, (version, format!("t{version}"))))
            .collect();
        prop_assert_eq!(server, expected);
    }
}
