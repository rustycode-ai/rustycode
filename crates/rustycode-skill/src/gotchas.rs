//! Gotcha warnings surfaced during skill activation.
//!
//! A skill can expose a short list of failure-mode reminders so the caller can
//! present them when the skill is activated.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GotchaList {
    items: Vec<String>,
}

impl GotchaList {
    pub fn from_list(raw: &[String]) -> Self {
        let items = raw
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        Self { items }
    }

    pub const fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub const fn len(&self) -> usize {
        self.items.len()
    }

    pub fn as_slice(&self) -> &[String] {
        &self.items
    }

    pub fn surface_for_context(&self, context: &str) -> Vec<String> {
        let context_lower = context.to_lowercase();
        self.items
            .iter()
            .filter(|item| {
                let item_lower = item.to_lowercase();
                context_lower.contains(&item_lower) || item_lower.contains(&context_lower)
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gotchas_from_list_filters_empty() {
        let gotchas = GotchaList::from_list(&[
            "watch for trailing commas".to_string(),
            " ".to_string(),
            "avoid blocking IO".to_string(),
        ]);
        assert_eq!(gotchas.len(), 2);
        assert!(!gotchas.is_empty());
    }

    #[test]
    fn gotchas_surface_for_context_matches_related_items() {
        let gotchas = GotchaList::from_list(&[
            "trailing commas".to_string(),
            "blocking IO".to_string(),
            "permissions".to_string(),
        ]);

        let surfaced = gotchas.surface_for_context("avoid blocking IO in writes");
        assert_eq!(surfaced, vec!["blocking IO".to_string()]);
    }
}
