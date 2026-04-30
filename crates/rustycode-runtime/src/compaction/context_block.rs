//! Context block for assembling always-present context zones.
//!
//! [`ContextZone`] defines the interface for a renderable, named section of the
//! prompt context. [`SessionContextBlock`] assembles four zones (environment,
//! session state, tools, multi-agent) into a single XML-wrapped string with
//! caching and token estimation.

/// A named, renderable section of the session context.
///
/// Implementations provide zone content (e.g. system prompt, tool definitions),
/// report staleness, and estimate their token cost. The trait is `Send + Sync`
/// so zones can be shared across async boundaries.
pub trait ContextZone: Send + Sync {
    /// Render the zone content as a plain string.
    fn render(&self) -> String;

    /// Whether the zone content has changed since the last render.
    fn is_stale(&self) -> bool;

    /// Rough token estimate for the rendered content.
    fn estimated_tokens(&self) -> usize;

    /// Human-readable zone identifier (e.g. "environment", "tools").
    fn name(&self) -> &str;
}

/// Simple [`ContextZone`] backed by a static `String`.
///
/// Useful for testing and for zones whose content does not change at runtime.
pub struct StringZone {
    name: String,
    content: String,
}

impl StringZone {
    /// Create a new static zone with the given name and content.
    pub fn new(name: &str, content: &str) -> Self {
        Self {
            name: name.to_string(),
            content: content.to_string(),
        }
    }
}

impl ContextZone for StringZone {
    fn render(&self) -> String {
        self.content.clone()
    }

    fn is_stale(&self) -> bool {
        false
    }

    fn estimated_tokens(&self) -> usize {
        self.content.len() / 4
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Assembles four context zones into a single XML-wrapped block.
///
/// On the first call to [`render`](Self::render), all four zones are rendered
/// and the result is cached. Subsequent calls return the cached string until
/// [`invalidate`](Self::invalidate) is called.
///
/// # Format
///
/// ```xml
/// <always-present-context>
/// <zone name="environment">
/// {environment.render()}
/// </zone>
/// <zone name="session-state">
/// {session_state.render()}
/// </zone>
/// <zone name="tools">
/// {tools.render()}
/// </zone>
/// <zone name="multi-agent">
/// {multi_agent.render()}
/// </zone>
/// </always-present-context>
/// ```
pub struct SessionContextBlock {
    environment: Box<dyn ContextZone>,
    session_state: Box<dyn ContextZone>,
    tools: Box<dyn ContextZone>,
    multi_agent: Box<dyn ContextZone>,
    cached_render: Option<String>,
    token_count: usize,
}

impl SessionContextBlock {
    /// Create a new context block from four zone providers.
    pub fn new(
        environment: Box<dyn ContextZone>,
        session_state: Box<dyn ContextZone>,
        tools: Box<dyn ContextZone>,
        multi_agent: Box<dyn ContextZone>,
    ) -> Self {
        Self {
            environment,
            session_state,
            tools,
            multi_agent,
            cached_render: None,
            token_count: 0,
        }
    }

    /// Render all zones wrapped in XML tags, caching the result.
    ///
    /// Only re-renders if the cache has been invalidated (or this is the
    /// first call). Updates the cached token count on render.
    pub fn render(&mut self) -> String {
        if let Some(ref cached) = self.cached_render {
            return cached.clone();
        }

        let rendered = format!(
            "<always-present-context>\n\
             <zone name=\"{}\">\n\
             {}\n\
             </zone>\n\
             <zone name=\"{}\">\n\
             {}\n\
             </zone>\n\
             <zone name=\"{}\">\n\
             {}\n\
             </zone>\n\
             <zone name=\"{}\">\n\
             {}\n\
             </zone>\n\
             </always-present-context>",
            self.environment.name(),
            self.environment.render(),
            self.session_state.name(),
            self.session_state.render(),
            self.tools.name(),
            self.tools.render(),
            self.multi_agent.name(),
            self.multi_agent.render(),
        );

        self.token_count = rendered.len() / 4;
        self.cached_render = Some(rendered.clone());
        rendered
    }

    /// Return the cached token estimate (total rendered chars / 4).
    ///
    /// Returns 0 if [`render`](Self::render) has not been called yet.
    pub fn token_count(&self) -> usize {
        self.token_count
    }

    /// Clear the cached render, forcing a full re-render on the next
    /// [`render`](Self::render) call.
    pub fn invalidate(&mut self) {
        self.cached_render = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_zone(name: &str, content: &str) -> Box<dyn ContextZone> {
        Box::new(StringZone::new(name, content))
    }

    fn test_block() -> SessionContextBlock {
        SessionContextBlock::new(
            test_zone("environment", "os=linux"),
            test_zone("session-state", "turn=5"),
            test_zone("tools", "bash,read,write"),
            test_zone("multi-agent", "agents=2"),
        )
    }

    #[test]
    fn xml_wrapped_output() {
        let mut block = test_block();
        let output = block.render();

        assert!(
            output.starts_with("<always-present-context>"),
            "output should start with outer XML tag"
        );
        assert!(
            output.ends_with("</always-present-context>"),
            "output should end with closing outer XML tag"
        );
        assert!(output.contains(r#"<zone name="environment">"#));
        assert!(output.contains(r#"<zone name="session-state">"#));
        assert!(output.contains(r#"<zone name="tools">"#));
        assert!(output.contains(r#"<zone name="multi-agent">"#));
        assert!(output.contains("os=linux"));
        assert!(output.contains("turn=5"));
        assert!(output.contains("bash,read,write"));
        assert!(output.contains("agents=2"));
    }

    #[test]
    fn caching_returns_same_string() {
        let mut block = test_block();
        let first = block.render();
        let second = block.render();
        assert_eq!(
            first, second,
            "second render should return the identical cached string"
        );
    }

    #[test]
    fn token_count_estimate() {
        let mut block = test_block();
        let output = block.render();
        let expected = output.len() / 4;
        assert_eq!(
            block.token_count(),
            expected,
            "token_count should equal rendered length / 4"
        );
    }

    #[test]
    fn invalidate_forces_rerender() {
        let mut block = test_block();
        let first = block.render();
        assert!(block.cached_render.is_some());

        block.invalidate();
        assert!(
            block.cached_render.is_none(),
            "invalidate should clear the cache"
        );

        // Re-render with the same zones produces the same content
        // but the cache was definitely rebuilt (no stale reference).
        let second = block.render();
        assert_eq!(first, second, "rerender with same zones should match");
        assert!(block.cached_render.is_some());
    }

    #[test]
    fn zone_stale_until_rendered() {
        let zone = StringZone::new("test", "content");
        assert!(!zone.is_stale(), "StringZone should never report stale");
    }

    #[test]
    fn string_zone_token_estimate() {
        let content = "a".repeat(40);
        let zone = StringZone::new("demo", &content);
        assert_eq!(
            zone.estimated_tokens(),
            10,
            "40-char string should estimate 10 tokens (len/4)"
        );
    }
}
