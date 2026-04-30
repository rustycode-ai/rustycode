use std::path::PathBuf;

fn main() {
    println!("Testing TUI initialization...");

    let cwd = PathBuf::from(".");

    println!("Current directory: {:?}", cwd);
    println!("Attempting to initialize TUI...");

    match rustycode_tui::run(cwd, false) {
        Ok(_) => println!("TUI started successfully"),
        Err(e) => eprintln!("TUI failed to start: {:?}", e),
    }
}
