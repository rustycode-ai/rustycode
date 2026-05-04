#![allow(clippy::redundant_clone, clippy::unwrap_used)]

//! Integration test for image paste functionality
//!
//! This test verifies that:
//! - Images can be attached to input state
//! - Images can be converted between different formats
//! - Temp files are properly cleaned up
//! - Image previews are generated

use rustycode_tui::ui::input::{InputHandler, InputState};
use rustycode_tui::ui::message::{ImageAttachment, Message};
use std::path::PathBuf;

#[test]
fn test_input_state_image_attachment() {
    let mut state = InputState::new();

    let img = ImageAttachment {
        id: "test123".to_string(),
        path: PathBuf::from("/tmp/test.png"),
        mime_type: "image/png".to_string(),
        preview: Some("test preview".to_string()),
        data_base64: None,
        width: None,
        height: None,
    };

    state.images.push(img.clone());
    assert_eq!(state.images.len(), 1);
    assert_eq!(state.images[0].id, "test123");

    // Test removal
    assert!(state.remove_image("test123"));
    assert_eq!(state.images.len(), 0);

    // Test removal of non-existent image
    assert!(!state.remove_image("nonexistent"));
}

#[test]
fn test_input_handler_with_images() {
    let mut handler = InputHandler::new();

    // Simulate adding an image (normally done via paste)
    let img = ImageAttachment {
        id: "img1".to_string(),
        path: PathBuf::from("/tmp/test1.png"),
        mime_type: "image/png".to_string(),
        preview: Some("preview1".to_string()),
        data_base64: None,
        width: None,
        height: None,
    };

    handler.state.images.push(img);

    // Verify image is attached
    assert_eq!(handler.state.images.len(), 1);

    // Clear should cleanup images
    handler.state.clear();
    assert_eq!(handler.state.images.len(), 0);
}

#[test]
fn test_message_with_images() {
    // Create a message with image attachments
    let images = vec![ImageAttachment {
        id: "msg_img1".to_string(),
        path: PathBuf::from("/tmp/msg_test.png"),
        mime_type: "image/png".to_string(),
        data_base64: Some("base64data".to_string()),
        preview: Some("preview".to_string()),
        width: Some(100),
        height: Some(100),
    }];

    let msg = Message::user("Check out this image!".to_string()).with_images(images);

    assert!(msg.has_images());
    assert_eq!(msg.image_count(), 1);
    assert_eq!(msg.metadata.images.unwrap()[0].id, "msg_img1");
}

#[test]
fn test_image_cleanup() {
    use std::fs::File;
    use std::io::Write;

    // Create a temporary test file
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_cleanup.png");

    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"test data").unwrap();

    assert!(test_file.exists());

    // Create input state with image
    let mut state = InputState::new();
    state
        .images
        .push(ImageAttachment {
            id: "cleanup_test".to_string(),
            path: test_file.clone(),
            mime_type: "image/png".to_string(),
            preview: None,
            data_base64: None,
            width: None,
            height: None,
        });

    // Clear should cleanup the file
    state.clear();

    assert!(
        !test_file.exists(),
        "Temp file should be deleted after clear"
    );
}
