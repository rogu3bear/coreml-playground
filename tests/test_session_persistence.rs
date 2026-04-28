//! Integration tests for SessionStore — validates CRUD operations,
//! message persistence, ordering, and cascade deletion.

#![cfg(feature = "ssr")]

use coreml_playground::types::*;

// Re-export the server module which is only available under the `ssr` feature.
use coreml_playground::server::session_store::SessionStore;

/// Helper: create a SessionStore backed by a unique temp file.
fn temp_store() -> (SessionStore, tempfile::NamedTempFile) {
    let tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
    let path = tmp.path().to_str().expect("non-UTF-8 temp path");
    let store = SessionStore::new(path).expect("failed to open SessionStore");
    (store, tmp)
}

/// Helper: build a ChatMessage with the given role and content.
fn make_msg(role: MessageRole, content: MessageContent) -> ChatMessage {
    ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role,
        content,
        timestamp: chrono::Utc::now().timestamp(),
        inference_ms: None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_create_session() {
    let (store, _tmp) = temp_store();

    let session = store
        .create_session("model-abc", "TestModel")
        .expect("create_session should succeed");

    // ID should be a valid UUID (36 chars with hyphens).
    assert_eq!(
        session.id.len(),
        36,
        "session id should be a UUID string (36 chars), got '{}'",
        session.id
    );

    assert_eq!(session.model_id, "model-abc", "model_id should match input");
    assert_eq!(
        session.model_name, "TestModel",
        "model_name should match input"
    );

    // Timestamps should be recent (within the last 5 seconds).
    let now = chrono::Utc::now().timestamp();
    assert!(
        (now - session.created_at).abs() < 5,
        "created_at should be within 5 seconds of now"
    );
    assert!(
        (now - session.updated_at).abs() < 5,
        "updated_at should be within 5 seconds of now"
    );

    assert_eq!(
        session.message_count, 0,
        "new session should have 0 messages"
    );
    assert!(
        session.preview.is_empty(),
        "new session should have empty preview"
    );
}

#[test]
fn test_list_sessions_ordering() {
    let (store, _tmp) = temp_store();

    // Create three sessions with sleeps between them so `updated_at`
    // (which is stored as whole seconds via `chrono::Utc::now().timestamp()`)
    // differs across sessions. We sleep just over 1 second to guarantee a
    // distinct timestamp for each.
    let s1 = store
        .create_session("m1", "First")
        .expect("create session 1");
    std::thread::sleep(std::time::Duration::from_millis(1100));

    let s2 = store
        .create_session("m2", "Second")
        .expect("create session 2");
    std::thread::sleep(std::time::Duration::from_millis(1100));

    let s3 = store
        .create_session("m3", "Third")
        .expect("create session 3");

    let sessions = store.list_sessions().expect("list_sessions should succeed");
    assert_eq!(sessions.len(), 3, "should have 3 sessions");

    // Most-recently-updated first: s3 > s2 > s1.
    assert_eq!(
        sessions[0].id, s3.id,
        "first session in list should be the most recently created (s3)"
    );
    assert_eq!(sessions[1].id, s2.id, "second session in list should be s2");
    assert_eq!(sessions[2].id, s1.id, "third session in list should be s1");

    // Verify descending updated_at.
    assert!(
        sessions[0].updated_at >= sessions[1].updated_at,
        "sessions should be in descending updated_at order"
    );
    assert!(
        sessions[1].updated_at >= sessions[2].updated_at,
        "sessions should be in descending updated_at order"
    );
}

#[test]
fn test_add_and_get_messages() {
    let (store, _tmp) = temp_store();

    let session = store
        .create_session("model-x", "ModelX")
        .expect("create session");

    // Add 5 messages with varied content types.
    let messages = vec![
        make_msg(MessageRole::User, MessageContent::Text("Hello".into())),
        make_msg(
            MessageRole::Model,
            MessageContent::ModelOutput(serde_json::json!({"label": "Positive", "score": 0.95})),
        ),
        make_msg(
            MessageRole::User,
            MessageContent::Image {
                data_base64: "aGVsbG8=".into(),
                mime_type: "image/png".into(),
                caption: Some("a test image".into()),
            },
        ),
        make_msg(
            MessageRole::Model,
            MessageContent::Text("Classified!".into()),
        ),
        make_msg(
            MessageRole::System,
            MessageContent::Streaming {
                partial: "loading...".into(),
                done: false,
            },
        ),
    ];

    for msg in &messages {
        store
            .add_message(&session.id, msg)
            .expect("add_message should succeed");
    }

    let retrieved = store
        .get_messages(&session.id)
        .expect("get_messages should succeed");

    assert_eq!(retrieved.len(), 5, "should retrieve all 5 messages");

    // Verify ordering is by timestamp ascending.
    for i in 1..retrieved.len() {
        assert!(
            retrieved[i].timestamp >= retrieved[i - 1].timestamp,
            "messages should be ordered by timestamp ascending"
        );
    }

    // Verify first message content roundtripped.
    assert_eq!(
        retrieved[0].content.as_text(),
        Some("Hello"),
        "first message text should roundtrip correctly"
    );

    // Verify the model output roundtripped.
    if let MessageContent::ModelOutput(ref val) = retrieved[1].content {
        assert_eq!(
            val.get("label").and_then(|v| v.as_str()),
            Some("Positive"),
            "ModelOutput label should roundtrip"
        );
    } else {
        panic!("expected ModelOutput for second message");
    }

    // Verify image content roundtripped.
    if let MessageContent::Image {
        ref data_base64,
        ref mime_type,
        ref caption,
    } = retrieved[2].content
    {
        assert_eq!(
            data_base64, "aGVsbG8=",
            "image data_base64 should roundtrip"
        );
        assert_eq!(mime_type, "image/png", "image mime_type should roundtrip");
        assert_eq!(
            caption.as_deref(),
            Some("a test image"),
            "image caption should roundtrip"
        );
    } else {
        panic!("expected Image content for third message");
    }
}

#[test]
fn test_message_content_roundtrip() {
    let (store, _tmp) = temp_store();

    let session = store
        .create_session("model-img", "ImageModel")
        .expect("create session");

    let original = MessageContent::Image {
        data_base64: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=".into(),
        mime_type: "image/png".into(),
        caption: Some("A 1x1 red pixel".into()),
    };

    let msg = ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        content: original,
        timestamp: chrono::Utc::now().timestamp(),
        inference_ms: None,
    };

    store
        .add_message(&session.id, &msg)
        .expect("add_message should succeed");

    let retrieved = store
        .get_messages(&session.id)
        .expect("get_messages should succeed");

    assert_eq!(retrieved.len(), 1, "should have exactly 1 message");

    if let MessageContent::Image {
        ref data_base64,
        ref mime_type,
        ref caption,
    } = retrieved[0].content
    {
        assert_eq!(
            data_base64,
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
            "base64 data should survive DB roundtrip"
        );
        assert_eq!(
            mime_type, "image/png",
            "mime_type should survive DB roundtrip"
        );
        assert_eq!(
            caption.as_deref(),
            Some("A 1x1 red pixel"),
            "caption should survive DB roundtrip"
        );
    } else {
        panic!(
            "expected Image content after DB roundtrip, got {:?}",
            retrieved[0].content
        );
    }
}

#[test]
fn test_delete_session_cascades() {
    let (store, _tmp) = temp_store();

    let session = store
        .create_session("model-del", "DeleteMe")
        .expect("create session");

    // Add 3 messages.
    for i in 0..3 {
        let msg = make_msg(
            MessageRole::User,
            MessageContent::Text(format!("message {i}")),
        );
        store
            .add_message(&session.id, &msg)
            .expect("add_message should succeed");
    }

    // Confirm messages exist.
    let before = store
        .get_messages(&session.id)
        .expect("get_messages before delete");
    assert_eq!(before.len(), 3, "should have 3 messages before delete");

    // Delete the session.
    store
        .delete_session(&session.id)
        .expect("delete_session should succeed");

    // Session should be gone from list.
    let sessions = store.list_sessions().expect("list_sessions after delete");
    assert!(
        sessions.iter().all(|s| s.id != session.id),
        "deleted session should not appear in list_sessions"
    );

    // Messages should also be gone.
    let after = store
        .get_messages(&session.id)
        .expect("get_messages after delete");
    assert!(
        after.is_empty(),
        "messages for deleted session should be empty, got {} messages",
        after.len()
    );
}

#[test]
fn test_session_preview() {
    let (store, _tmp) = temp_store();

    let session = store
        .create_session("model-prev", "PreviewModel")
        .expect("create session");

    // Add a user text message that should appear as the preview.
    let msg = make_msg(
        MessageRole::User,
        MessageContent::Text("What breed is this dog?".into()),
    );
    store
        .add_message(&session.id, &msg)
        .expect("add_message should succeed");

    let sessions = store.list_sessions().expect("list_sessions");
    let found = sessions
        .iter()
        .find(|s| s.id == session.id)
        .expect("session should appear in list");

    assert!(
        found.preview.contains("What breed is this dog?"),
        "session preview should contain the user's message text, got '{}'",
        found.preview
    );

    assert_eq!(
        found.message_count, 1,
        "message_count should be 1 after adding one message"
    );
}

#[test]
fn test_empty_database() {
    let (store, _tmp) = temp_store();

    // Fresh store should have no sessions.
    let sessions = store.list_sessions().expect("list_sessions on empty db");
    assert!(
        sessions.is_empty(),
        "fresh database should return empty session list"
    );

    // get_messages for a nonexistent session should return an empty vec.
    let messages = store
        .get_messages("nonexistent-session-id")
        .expect("get_messages for nonexistent session should not error");
    assert!(
        messages.is_empty(),
        "get_messages for nonexistent session should return empty vec"
    );
}

// ---------------------------------------------------------------------------
// get_session
// ---------------------------------------------------------------------------

#[test]
fn test_get_session_found() {
    let (store, _tmp) = temp_store();

    let created = store
        .create_session("model-get", "GetModel")
        .expect("create session");

    let found = store
        .get_session(&created.id)
        .expect("get_session should succeed")
        .expect("session should be found");

    assert_eq!(found.id, created.id);
    assert_eq!(found.model_id, "model-get");
    assert_eq!(found.model_name, "GetModel");
    assert_eq!(found.message_count, 0);
}

#[test]
fn test_get_session_not_found() {
    let (store, _tmp) = temp_store();

    let result = store
        .get_session("nonexistent-id")
        .expect("get_session should not error for missing id");

    assert!(result.is_none(), "nonexistent session should return None");
}

// ---------------------------------------------------------------------------
// rename_session
// ---------------------------------------------------------------------------

#[test]
fn test_rename_session_success() {
    let (store, _tmp) = temp_store();

    let session = store
        .create_session("model-rn", "RenameModel")
        .expect("create session");

    // Small sleep so updated_at can advance.
    std::thread::sleep(std::time::Duration::from_millis(1100));

    store
        .rename_session(&session.id, "My Custom Name")
        .expect("rename_session should succeed");

    // Verify the name appears in list_sessions preview.
    let sessions = store.list_sessions().expect("list_sessions");
    let found = sessions
        .iter()
        .find(|s| s.id == session.id)
        .expect("session should still exist after rename");

    assert_eq!(
        found.preview, "My Custom Name",
        "preview should reflect the new display_name"
    );

    // Verify updated_at advanced.
    assert!(
        found.updated_at > session.updated_at,
        "updated_at should advance after rename (before={}, after={})",
        session.updated_at,
        found.updated_at
    );
}

#[test]
fn test_rename_session_not_found() {
    let (store, _tmp) = temp_store();

    let result = store.rename_session("nonexistent-id", "New Name");
    assert!(
        result.is_err(),
        "renaming a nonexistent session should return an error"
    );
    let err = result.unwrap_err();
    assert!(err.contains("not found"), "got: {}", err);
}

#[test]
fn test_rename_session_empty_rejected() {
    let (store, _tmp) = temp_store();
    let session = store
        .create_session("model-1", "Test Model")
        .expect("create session");
    let err = store.rename_session(&session.id, "").unwrap_err();
    assert!(err.contains("empty"), "got: {}", err);
}

#[test]
fn test_rename_session_whitespace_rejected() {
    let (store, _tmp) = temp_store();
    let session = store
        .create_session("model-1", "Test Model")
        .expect("create session");
    let err = store.rename_session(&session.id, "   ").unwrap_err();
    assert!(err.contains("empty"), "got: {}", err);
}

#[test]
fn test_rename_session_trims_whitespace() {
    let (store, _tmp) = temp_store();
    let session = store
        .create_session("model-1", "Test Model")
        .expect("create session");
    store
        .rename_session(&session.id, "  New Name  ")
        .expect("rename with surrounding whitespace should succeed");
    let updated = store
        .get_session(&session.id)
        .expect("get_session should succeed")
        .expect("session should exist");
    assert_eq!(updated.preview, "New Name");
}

// ---------------------------------------------------------------------------
// save_comparison + get_comparisons
// ---------------------------------------------------------------------------

#[test]
fn test_comparison_roundtrip() {
    let (store, _tmp) = temp_store();

    let session = store
        .create_session("model-cmp", "CompareModel")
        .expect("create session");

    // Save two comparisons.
    let id1 = store
        .save_comparison(
            &session.id,
            r#"{"text":"hello"}"#,
            "model-a",
            "model-b",
            Some("msg-left-1"),
            Some("msg-right-1"),
        )
        .expect("save_comparison 1");

    std::thread::sleep(std::time::Duration::from_millis(1100));

    let id2 = store
        .save_comparison(
            &session.id,
            r#"{"text":"world"}"#,
            "model-c",
            "model-d",
            None,
            None,
        )
        .expect("save_comparison 2");

    // Retrieve and verify.
    let comparisons = store
        .get_comparisons(&session.id)
        .expect("get_comparisons should succeed");

    assert_eq!(comparisons.len(), 2, "should have 2 comparisons");

    // Ordered by created_at ASC, so id1 first.
    assert_eq!(comparisons[0].id, id1);
    assert_eq!(comparisons[0].input_json, r#"{"text":"hello"}"#);
    assert_eq!(comparisons[0].left_model_id, "model-a");
    assert_eq!(comparisons[0].right_model_id, "model-b");
    assert_eq!(
        comparisons[0].left_message_id.as_deref(),
        Some("msg-left-1")
    );
    assert_eq!(
        comparisons[0].right_message_id.as_deref(),
        Some("msg-right-1")
    );

    assert_eq!(comparisons[1].id, id2);
    assert_eq!(comparisons[1].input_json, r#"{"text":"world"}"#);
    assert_eq!(comparisons[1].left_model_id, "model-c");
    assert_eq!(comparisons[1].right_model_id, "model-d");
    assert!(comparisons[1].left_message_id.is_none());
    assert!(comparisons[1].right_message_id.is_none());

    // Ordering: first comparison should have earlier timestamp.
    assert!(
        comparisons[0].created_at <= comparisons[1].created_at,
        "comparisons should be ordered by created_at ASC"
    );
}

#[test]
fn test_delete_session_cleans_comparisons() {
    let (store, _tmp) = temp_store();

    let session = store
        .create_session("model-cmp-del", "CmpDeleteModel")
        .expect("create session");

    // Save two comparisons.
    store
        .save_comparison(
            &session.id,
            r#"{"text":"hello"}"#,
            "model-a",
            "model-b",
            None,
            None,
        )
        .expect("save_comparison 1");

    store
        .save_comparison(
            &session.id,
            r#"{"text":"world"}"#,
            "model-c",
            "model-d",
            None,
            None,
        )
        .expect("save_comparison 2");

    // Verify comparisons exist.
    let before = store
        .get_comparisons(&session.id)
        .expect("get_comparisons before delete");
    assert_eq!(before.len(), 2, "should have 2 comparisons before delete");

    // Delete the session.
    store
        .delete_session(&session.id)
        .expect("delete_session should succeed");

    // Comparisons should be gone.
    let after = store
        .get_comparisons(&session.id)
        .expect("get_comparisons after delete");
    assert!(
        after.is_empty(),
        "comparisons for deleted session should be empty, got {} comparisons",
        after.len()
    );
}

#[test]
fn test_get_comparisons_empty() {
    let (store, _tmp) = temp_store();

    let session = store
        .create_session("model-empty-cmp", "EmptyCompare")
        .expect("create session");

    let comparisons = store
        .get_comparisons(&session.id)
        .expect("get_comparisons should succeed");

    assert!(
        comparisons.is_empty(),
        "session with no comparisons should return empty vec"
    );
}
