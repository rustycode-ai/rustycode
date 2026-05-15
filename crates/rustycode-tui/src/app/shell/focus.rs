//! Focus management for the AppShell feature system.
//!
//! `FocusRing` maintains an ordered list of focusable surface IDs and provides
//! cycling focus semantics (next/prev/set). Used by `AppShell` to route input
//! events to the currently focused feature.

use crate::app::features::SurfaceId;

/// Ordered ring of focusable surface IDs.
///
/// Supports cycling (`focus_next`/`focus_prev`), direct set (`focus_set`),
/// and query (`focused`). Empty rings are valid — all operations become no-ops.
#[derive(Debug, Clone)]
pub struct FocusRing {
    /// Ordered list of surface IDs that can receive focus.
    order: Vec<SurfaceId>,
    /// Index into `order` for the currently focused surface, if any.
    focused_idx: Option<usize>,
}

impl FocusRing {
    /// Create an empty focus ring.
    pub fn new() -> Self {
        Self {
            order: Vec::new(),
            focused_idx: None,
        }
    }

    /// Add a surface ID to the end of the focus ring.
    /// If it's already present, this is a no-op.
    pub fn add(&mut self, surface: SurfaceId) {
        if !self.order.contains(&surface) {
            self.order.push(surface);
            // If this is the first entry and nothing is focused, auto-focus it.
            if self.order.len() == 1 && self.focused_idx.is_none() {
                self.focused_idx = Some(0);
            }
        }
    }

    /// Remove a surface ID from the focus ring.
    /// If the removed surface was focused, focus moves to the next (or prev if last).
    pub fn remove(&mut self, surface: SurfaceId) {
        if let Some(pos) = self.order.iter().position(|&s| s == surface) {
            self.order.remove(pos);
            if self.order.is_empty() {
                self.focused_idx = None;
            } else if let Some(idx) = self.focused_idx {
                if pos < idx {
                    // Removed before current — shift index back
                    self.focused_idx = Some(idx - 1);
                } else if pos == idx {
                    // Removed the focused surface — wrap to next (or first)
                    self.focused_idx = Some(if idx >= self.order.len() { 0 } else { idx });
                }
                // If pos > idx, no change needed
            }
        }
    }

    /// Move focus to the next surface in the ring (wraps around).
    /// Returns the newly focused surface, or `None` if the ring is empty.
    pub fn focus_next(&mut self) -> Option<SurfaceId> {
        if self.order.is_empty() {
            return None;
        }
        let next = match self.focused_idx {
            Some(idx) => (idx + 1) % self.order.len(),
            None => 0,
        };
        self.focused_idx = Some(next);
        Some(self.order[next])
    }

    /// Move focus to the previous surface in the ring (wraps around).
    /// Returns the newly focused surface, or `None` if the ring is empty.
    pub fn focus_prev(&mut self) -> Option<SurfaceId> {
        if self.order.is_empty() {
            return None;
        }
        let prev = match self.focused_idx {
            Some(idx) => {
                if idx == 0 {
                    self.order.len() - 1
                } else {
                    idx - 1
                }
            }
            None => 0,
        };
        self.focused_idx = Some(prev);
        Some(self.order[prev])
    }

    /// Set focus directly to a specific surface.
    /// Returns `true` if the surface exists in the ring, `false` otherwise.
    pub fn focus_set(&mut self, surface: SurfaceId) -> bool {
        if let Some(idx) = self.order.iter().position(|&s| s == surface) {
            self.focused_idx = Some(idx);
            true
        } else {
            false
        }
    }

    /// Get the currently focused surface, if any.
    pub fn focused(&self) -> Option<SurfaceId> {
        self.focused_idx
            .and_then(|idx| self.order.get(idx).copied())
    }

    /// Get the number of surfaces in the ring.
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Check if the ring is empty.
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Get an iterator over the ordered surface IDs.
    pub fn iter(&self) -> impl Iterator<Item = SurfaceId> + '_ {
        self.order.iter().copied()
    }

    /// Clear all surfaces and reset focus.
    pub fn clear(&mut self) {
        self.order.clear();
        self.focused_idx = None;
    }
}

impl Default for FocusRing {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sid(name: &'static str) -> SurfaceId {
        SurfaceId::new(name)
    }

    #[test]
    fn empty_ring_has_no_focus() {
        let ring = FocusRing::new();
        assert!(ring.is_empty());
        assert_eq!(ring.focused(), None);
    }

    #[test]
    fn default_is_empty() {
        let ring = FocusRing::default();
        assert!(ring.is_empty());
    }

    #[test]
    fn add_first_auto_focuses() {
        let mut ring = FocusRing::new();
        ring.add(sid("chat"));
        assert_eq!(ring.focused(), Some(sid("chat")));
    }

    #[test]
    fn add_second_keeps_focus() {
        let mut ring = FocusRing::new();
        ring.add(sid("chat"));
        ring.add(sid("sidebar"));
        assert_eq!(ring.focused(), Some(sid("chat")));
    }

    #[test]
    fn add_duplicate_is_noop() {
        let mut ring = FocusRing::new();
        ring.add(sid("chat"));
        ring.add(sid("chat"));
        assert_eq!(ring.len(), 1);
    }

    #[test]
    fn focus_next_cycles() {
        let mut ring = FocusRing::new();
        ring.add(sid("a"));
        ring.add(sid("b"));
        ring.add(sid("c"));

        assert_eq!(ring.focused(), Some(sid("a")));
        assert_eq!(ring.focus_next(), Some(sid("b")));
        assert_eq!(ring.focus_next(), Some(sid("c")));
        assert_eq!(ring.focus_next(), Some(sid("a"))); // wraps
    }

    #[test]
    fn focus_prev_cycles() {
        let mut ring = FocusRing::new();
        ring.add(sid("a"));
        ring.add(sid("b"));
        ring.add(sid("c"));

        assert_eq!(ring.focused(), Some(sid("a")));
        assert_eq!(ring.focus_prev(), Some(sid("c"))); // wraps back
        assert_eq!(ring.focus_prev(), Some(sid("b")));
        assert_eq!(ring.focus_prev(), Some(sid("a")));
    }

    #[test]
    fn focus_set_existing() {
        let mut ring = FocusRing::new();
        ring.add(sid("a"));
        ring.add(sid("b"));
        ring.add(sid("c"));

        assert!(ring.focus_set(sid("b")));
        assert_eq!(ring.focused(), Some(sid("b")));
    }

    #[test]
    fn focus_set_missing_returns_false() {
        let mut ring = FocusRing::new();
        ring.add(sid("a"));
        assert!(!ring.focus_set(sid("z")));
        assert_eq!(ring.focused(), Some(sid("a"))); // unchanged
    }

    #[test]
    fn remove_focused_moves_to_next() {
        let mut ring = FocusRing::new();
        ring.add(sid("a"));
        ring.add(sid("b"));
        ring.add(sid("c"));

        ring.focus_set(sid("b"));
        ring.remove(sid("b"));
        assert_eq!(ring.focused(), Some(sid("c")));
    }

    #[test]
    fn remove_focused_last_wraps_to_first() {
        let mut ring = FocusRing::new();
        ring.add(sid("a"));
        ring.add(sid("b"));

        ring.focus_set(sid("b"));
        ring.remove(sid("b"));
        assert_eq!(ring.focused(), Some(sid("a")));
    }

    #[test]
    fn remove_before_focused_adjusts_index() {
        let mut ring = FocusRing::new();
        ring.add(sid("a"));
        ring.add(sid("b"));
        ring.add(sid("c"));

        ring.focus_set(sid("c")); // idx=2
        ring.remove(sid("a")); // remove idx=0, "c" shifts to idx=1
        assert_eq!(ring.focused(), Some(sid("c")));
        assert_eq!(ring.len(), 2);
    }

    #[test]
    fn remove_only_element_clears_focus() {
        let mut ring = FocusRing::new();
        ring.add(sid("only"));
        ring.remove(sid("only"));
        assert!(ring.is_empty());
        assert_eq!(ring.focused(), None);
    }

    #[test]
    fn remove_nonexistent_is_noop() {
        let mut ring = FocusRing::new();
        ring.add(sid("a"));
        ring.remove(sid("z"));
        assert_eq!(ring.len(), 1);
        assert_eq!(ring.focused(), Some(sid("a")));
    }

    #[test]
    fn focus_next_on_empty_returns_none() {
        let mut ring = FocusRing::new();
        assert_eq!(ring.focus_next(), None);
        assert_eq!(ring.focus_prev(), None);
    }

    #[test]
    fn focus_next_unfocused_picks_first() {
        let mut ring = FocusRing::new();
        ring.add(sid("a"));
        ring.add(sid("b"));
        ring.focused_idx = None; // simulate no focus
        assert_eq!(ring.focus_next(), Some(sid("a")));
    }

    #[test]
    fn clear_resets() {
        let mut ring = FocusRing::new();
        ring.add(sid("a"));
        ring.add(sid("b"));
        ring.clear();
        assert!(ring.is_empty());
        assert_eq!(ring.focused(), None);
    }

    #[test]
    fn iter_returns_ordered() {
        let mut ring = FocusRing::new();
        ring.add(sid("c"));
        ring.add(sid("a"));
        ring.add(sid("b"));
        let items: Vec<_> = ring.iter().collect();
        assert_eq!(items, vec![sid("c"), sid("a"), sid("b")]);
    }

    #[test]
    fn single_element_cycles() {
        let mut ring = FocusRing::new();
        ring.add(sid("only"));
        assert_eq!(ring.focus_next(), Some(sid("only")));
        assert_eq!(ring.focus_prev(), Some(sid("only")));
    }
}
