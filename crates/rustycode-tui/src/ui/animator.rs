//! Animation system for status indicators
//!
//! Provides efficient 2-4 FPS animations for status indicators, pulsing cursors,
//! and progress indicators. Designed for low CPU usage.

use std::time::{Duration, Instant};

/// Current animation frame
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct AnimationFrame {
    /// Animated cursor character
    pub cursor: char,
    /// Animated dots (for loading text)
    pub dots: &'static str,
    /// Progress bar animation frame
    pub progress_frame: usize,
    /// Whether animation is in "active" phase (for pulsing effects)
    pub is_active: bool,
}

impl AnimationFrame {
    /// Get a static frame (for reduced motion mode)
    pub fn static_frame() -> Self {
        Self {
            cursor: '⏳',
            dots: "...",
            progress_frame: 0,
            is_active: false,
        }
    }
}

/// Animation style for different indicator types
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum AnimationStyle {
    /// No animation (static)
    #[default]
    None,
    /// Pulsing cursor
    Pulsing,
    /// Cycling dots
    Dots,
    /// Progress bar
    Progress,
    /// Combined pulsing + dots
    PulsingDots,
}

/// Animator for status indicators
///
/// Runs at 2-4 FPS for efficiency (not 60 FPS). Updates animation frames
/// based on elapsed time since last frame.
#[non_exhaustive]
pub struct Animator {
    frame_count: usize,
    /// Last frame update time
    last_frame_time: Instant,
    /// Frame duration in milliseconds
    frame_duration_ms: u64,
    /// Reduced motion mode
    reduced_motion: bool,
}

impl Animator {
    /// Create a new animator
    ///
    pub fn new(target_fps: u32, reduced_motion: bool) -> Self {
        let frame_duration_ms = if target_fps == 0 {
            500u64 // Default to 2 FPS
        } else {
            1000u64 / target_fps as u64
        };

        // Initialize last_frame_time to far in the past so first update() always returns true
        let last_frame_time = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or(Instant::now());

        Self {
            frame_count: 0,
            last_frame_time,
            frame_duration_ms,
            reduced_motion,
        }
    }

    /// Create with default settings (3 FPS, animations enabled)
    pub fn default_enabled() -> Self {
        Self::new(3, false)
    }

    /// Create with reduced motion (static frames only)
    pub fn reduced_motion() -> Self {
        Self::new(3, true)
    }

    /// Update the animator
    ///
    /// Returns true if the frame changed, false if not yet time for next frame.
    /// This should be called once per render loop iteration.
    pub fn update(&mut self) -> bool {
        if self.reduced_motion {
            return false; // Never animate in reduced motion mode
        }

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_frame_time);

        if elapsed >= Duration::from_millis(self.frame_duration_ms) {
            self.frame_count = self.frame_count.wrapping_add(1);
            self.last_frame_time = now;
            true
        } else {
            false
        }
    }

    /// Get current animation frame
    pub fn current_frame(&self) -> AnimationFrame {
        if self.reduced_motion {
            return AnimationFrame::static_frame();
        }

        // Cycle through 8 frames for variety
        let frame = self.frame_count % 8;

        AnimationFrame {
            cursor: match frame {
                0 | 2 | 4 | 6 => '⏳',
                _ => '⌛',
            },
            dots: match frame {
                0 => ".",
                1 => "..",
                2 => "...",
                3 => "....",
                4 => "...",
                5 => "..",
                _ => ".",
            },
            progress_frame: frame,
            is_active: frame < 4, // First half of cycle is "active"
        }
    }

    /// Get current frame count (for custom animations)
    pub fn frame_count(&self) -> usize {
        self.frame_count
    }

    pub fn is_reduced_motion(&self) -> bool {
        self.reduced_motion
    }

    /// Reset the animator
    pub fn reset(&mut self) {
        self.frame_count = 0;
        self.last_frame_time = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_animator_creation() {
        let animator = Animator::new(4, false);
        assert_eq!(animator.frame_duration_ms, 250);
        assert!(!animator.is_reduced_motion());
    }

    #[test]
    fn test_reduced_motion() {
        let animator = Animator::reduced_motion();
        assert!(animator.is_reduced_motion());
        let frame = animator.current_frame();
        assert_eq!(frame.cursor, '⏳');
        assert_eq!(frame.dots, "...");
        assert!(!frame.is_active);
    }

    #[test]
    fn test_animation_frame() {
        let mut animator = Animator::default_enabled();

        // Test multiple frames
        let frames: Vec<_> = (0..10)
            .map(|_| {
                animator.frame_count = animator.frame_count.wrapping_add(1);
                animator.current_frame()
            })
            .collect();

        // Verify we get different animations
        let cursors: Vec<char> = frames.iter().map(|f| f.cursor).collect();
        assert!(cursors.contains(&'⏳'));
        assert!(cursors.contains(&'⌛'));

        // Verify dots cycle
        let dots: Vec<&str> = frames.iter().map(|f| f.dots).collect();
        assert!(dots.contains(&"."));
        assert!(dots.contains(&"..."));
        assert!(dots.contains(&"...."));
    }

    #[test]
    fn test_update_timing() {
        let mut animator = Animator::new(10, false); // 10 FPS = 100ms

        // First update should happen immediately
        assert!(animator.update());
        let count1 = animator.frame_count();

        // Immediate update should not trigger
        assert!(!animator.update());
        assert_eq!(animator.frame_count(), count1);

        // Wait for frame duration
        thread::sleep(Duration::from_millis(110));
        assert!(animator.update());
        assert_eq!(animator.frame_count(), count1 + 1);
    }

    #[test]
    fn test_static_frame() {
        let frame = AnimationFrame::static_frame();
        assert_eq!(frame.cursor, '⏳');
        assert_eq!(frame.dots, "...");
        assert_eq!(frame.progress_frame, 0);
        assert!(!frame.is_active);
    }

    #[test]
    fn test_reset() {
        let mut animator = Animator::default_enabled();
        animator.frame_count = 100;
        animator.reset();
        assert_eq!(animator.frame_count(), 0);
    }
}
