//! Cross-platform browser URL opener for OAuth flows.
//!
//! Opens the user's default browser to the OAuth authorization URL.
//! Falls back to printing the URL for manual copy when no display is available.

use crate::AuthResult;
use std::process::Command;

/// Open a URL in the user's default browser.
///
/// Returns `Ok(())` if the browser was launched (or the command appeared to succeed).
/// Returns `Ok(())` with a printed URL fallback in headless/SSH/WSL environments.
pub fn open_url(url: &str) -> AuthResult<()> {
    // Check for a display environment (skip browser in headless/SSH/CI)
    if !display_available() {
        eprintln!("  No display detected. Open this URL manually:");
        eprintln!("  {url}");
        return Ok(());
    }

    let result = launch_browser(url);

    if result.is_err() {
        // Browser launch failed — fall back to printing
        eprintln!("  Could not open browser. Open this URL manually:");
        eprintln!("  {url}");
    }
    Ok(())
}

/// Launch the platform-appropriate browser command.
fn launch_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(url).spawn()?.wait()?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(url).spawn()?.wait()?;
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/c", "start", url])
            .spawn()?
            .wait()?;
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "platform not supported for browser opening",
        ));
    }

    Ok(())
}

/// Check whether a graphical display is available.
fn display_available() -> bool {
    // macOS always has a display (or Quartz virtual display)
    #[cfg(target_os = "macos")]
    {
        // In SSH sessions on macOS, there may be no GUI
        std::env::var("SSH_CONNECTION").is_err()
    }

    #[cfg(not(target_os = "macos"))]
    {
        #[cfg(target_os = "linux")]
        {
            std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok()
        }
        #[cfg(not(target_os = "linux"))]
        {
            // Windows or unknown — assume display available
            true
        }
    }
}

/// Check if we're running in a headless/container environment.
pub fn is_headless() -> bool {
    !display_available()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_available_returns_bool() {
        // Just verify it doesn't panic
        let _ = display_available();
    }

    #[test]
    fn is_headless_returns_bool() {
        let _ = is_headless();
    }

    #[test]
    #[ignore = "opens browser in GUI environments"]
    fn open_url_does_not_panic_on_any_url() {
        let _ = open_url("https://example.com/oauth?code=abc&state=xyz");
        let _ = open_url("http://localhost:9090/callback");
        let _ = open_url("");
    }

    #[test]
    #[ignore = "opens browser in GUI environments"]
    fn open_url_handles_special_chars() {
        let _ = open_url("https://example.com/auth?redirect_uri=http%3A%2F%2Flocalhost%3A9090%2Fcallback&scope=read+write");
    }
}
