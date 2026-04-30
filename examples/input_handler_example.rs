//! Example of how to integrate the InputHandler into a TUI application.
//!
//! This demonstrates the event loop integration pattern and UI rendering.

use crossterm::event::{KeyCode, KeyModifiers};
use rustycode_tui::ui::input::{ImageAttachment, InputAction, InputHandler, InputMode, InputState};

/// Example event loop integration
fn example_event_loop() {
    let mut input_handler = InputHandler::new();
    let mut needs_redraw = true;

    // Main event loop
    loop {
        if needs_redraw {
            render_input(&input_handler.state);
            needs_redraw = false;
        }

        // Read keyboard event (pseudo-code)
        // let key_event = read_key_event();

        // Example: User presses 'H'
        let action = input_handler.handle_key_event(KeyCode::Char('H'), KeyModifiers::NONE);

        match action {
            InputAction::SendMessage(lines) => {
                let content = lines.join("\n");
                let images = input_handler.state.images.clone();

                println!("Sending message: {}", content);
                println!("With {} images", images.len());

                // Send to LLM...
                // Clear input after sending
                input_handler.state.clear();
                needs_redraw = true;
            }
            InputAction::Consumed => {
                // Input changed, trigger redraw
                needs_redraw = true;
            }
            InputAction::Ignored => {
                // Pass to other handlers
            }
            InputAction::HistoryPrevious => {
                println!("Browse history backward");
            }
            InputAction::HistoryNext => {
                println!("Browse history forward");
            }
            InputAction::RemoveImage(id) => {
                input_handler.state.remove_image(&id);
                needs_redraw = true;
            }
            InputAction::SearchReverse => {
                println!("Search reverse");
            }
            InputAction::OpenCommandPalette => {
                println!("Open command palette");
            }
            InputAction::OpenSkillPalette => {
                println!("Open skill palette");
            }
        }
    }
}

/// Example UI rendering
fn render_input(state: &InputState) {
    // Mode indicator
    let mode_str = match state.mode {
        InputMode::SingleLine => "[SINGLE]",
        InputMode::MultiLine => "[MULTI]",
    };
    println!("{}", mode_str);

    // Show hint
    if state.mode == InputMode::SingleLine {
        println!("Option+Enter for multi-line");
    } else {
        println!("Option+Enter to send • Esc to exit multi-line");
    }

    // Show lines
    for (i, line) in state.lines.iter().enumerate() {
        if state.mode == InputMode::MultiLine {
            print!("{:>2} ", i + 1);
        }
        println!("{}", line);
    }

    // Show images
    if !state.images.is_empty() {
        println!("\nAttached images:");
        for img in &state.images {
            println!("📷 {} [x] remove", img.path.display());
            println!("{}", img.preview);
        }
    }

    // Show cursor position
    if state.mode == InputMode::MultiLine {
        println!(
            "Cursor: line {}, col {}",
            state.cursor_row + 1,
            state.cursor_col
        );
    }
}

/// Example: Paste handling integration
fn example_paste_integration() {
    let mut input_handler = InputHandler::new();

    // Simulate Ctrl+V paste
    let action = input_handler.handle_key_event(KeyCode::Char('v'), KeyModifiers::CONTROL);

    match action {
        InputAction::Consumed => {
            // Check what was pasted
            if !input_handler.state.images.is_empty() {
                println!("Pasted {} image(s)", input_handler.state.images.len());
            } else if !input_handler.state.lines.is_empty() {
                let text = input_handler.state.all_text();
                println!("Pasted text: {}", text);
            }
        }
        _ => {}
    }
}

/// Example: Multi-line editing workflow
fn example_multiline_workflow() {
    let mut handler = InputHandler::new();

    // User types "Hello"
    handler.handle_key_event(KeyCode::Char('H'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('e'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('l'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('l'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('o'), KeyModifiers::NONE);

    // User presses Option+Enter to enter multi-line mode
    handler.handle_key_event(KeyCode::Enter, KeyModifiers::ALT);
    assert_eq!(handler.state.mode, InputMode::MultiLine);

    // User types "World"
    handler.handle_key_event(KeyCode::Char('W'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('o'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('r'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('l'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('d'), KeyModifiers::NONE);

    // User presses Enter to add newline
    handler.handle_key_event(KeyCode::Enter, KeyModifiers::NONE);

    // User types "!"
    handler.handle_key_event(KeyCode::Char('!'), KeyModifiers::NONE);

    // Now we have:
    // Line 1: Hello
    // Line 2: World
    // Line 3: !
    assert_eq!(handler.state.lines.len(), 3);

    // User presses Option+Enter to send
    let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::ALT);
    assert!(matches!(action, InputAction::SendMessage(_)));
}

/// Example: Image attachment workflow
fn example_image_workflow() {
    let mut handler = InputHandler::new();

    // User pastes an image (Ctrl+V)
    handler.handle_key_event(KeyCode::Char('v'), KeyModifiers::CONTROL);

    // Check if image was attached
    if let Some(img) = handler.state.images.first() {
        println!("Image attached: {}", img.path.display());
        println!("Preview:\n{}", img.preview);
        println!("MIME type: {}", img.mime_type);

        // User can remove image by ID
        let id = img.id.clone();
        handler.state.remove_image(&id);
    }
}

/// Example: Exiting multi-line mode
fn example_exit_multiline() {
    let mut handler = InputHandler::new();

    // Enter multi-line mode
    handler.state.mode = InputMode::MultiLine;
    handler.state.lines = vec![
        "Line 1".to_string(),
        "Line 2".to_string(),
        "Line 3".to_string(),
    ];

    // User presses Esc to exit multi-line
    handler.handle_key_event(KeyCode::Esc, KeyModifiers::NONE);

    // Should collapse to single line
    assert_eq!(handler.state.mode, InputMode::SingleLine);
    assert_eq!(handler.state.lines.len(), 1);
    assert_eq!(handler.state.lines[0], "Line 1 Line 2 Line 3");
}

fn main() {
    println!("Input Handler Integration Examples");
    println!("===================================\n");

    println!("See function documentation for examples:");
    println!("- example_event_loop()");
    println!("- render_input()");
    println!("- example_paste_integration()");
    println!("- example_multiline_workflow()");
    println!("- example_image_workflow()");
    println!("- example_exit_multiline()");
}
