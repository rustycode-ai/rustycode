#!/bin/bash
# Test terminal configuration for crossterm

echo "=== Terminal Configuration Test ==="
echo ""

# Check if we're in a terminal
if [ -t 0 ]; then
    echo "✓ Stdin is a TTY"
else
    echo "✗ Stdin is NOT a TTY"
fi

if [ -t 1 ]; then
    echo "✓ Stdout is a TTY"
else
    echo "✗ Stdout is NOT a TTY"
fi

if [ -t 2 ]; then
    echo "✓ Stderr is a TTY"
else
    echo "✗ Stderr is NOT a TTY"
fi

echo ""
echo "=== Terminal Type ==="
echo "TERM=$TERM"

echo ""
echo "=== Testing crossterm event reading ==="
echo "This test will wait for a single keypress..."
echo "Press any key (the key will be displayed)"
echo ""

# Create a simple Rust test program
cat > /tmp/test_crossterm.rs <<'EOF'
use crossterm::event::{self, Event, KeyCode};
use std::time::Duration;

fn main() -> crossterm::Result<()> {
    println!("Waiting for key press (5 second timeout)...");

    // Enable raw mode
    crossterm::terminal::enable_raw_mode()?;

    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(5);

    while start.elapsed() < timeout {
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key_event) => {
                    println!("\r\nKey event received: {:?}", key_event);
                    crossterm::terminal::disable_raw_mode()?;
                    return Ok(());
                }
                Event::Mouse(mouse_event) => {
                    println!("\r\nMouse event received: {:?}", mouse_event);
                }
                Event::Resize(width, height) => {
                    println!("\r\nResize event: {}x{}", width, height);
                }
                _ => {}
            }
        }
    }

    // Disable raw mode
    crossterm::terminal::disable_raw_mode()?;

    println!("\r\nNo key event received within timeout");
    Ok(())
}
EOF

echo ""
echo "Compiling test program..."
if rustc +stable /tmp/test_crossterm.rs --edition 2021 -o /tmp/test_crossterm 2>&1; then
    echo "✓ Compilation successful"
    echo ""
    echo "Running test..."
    /tmp/test_crossterm
    echo ""
    echo "Test completed"
else
    echo "✗ Compilation failed"
fi
