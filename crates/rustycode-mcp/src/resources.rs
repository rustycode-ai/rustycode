//! Resource management for MCP

use crate::manager::McpServer;
use crate::types::*;
use crate::{McpError, McpResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Resource with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    /// Resource URI
    pub uri: String,
    /// Resource name
    pub name: String,
    /// Resource description
    pub description: String,
    pub mime_type: String,
    /// Server ID that provides this resource
    pub server_id: String,
}

impl From<McpResource> for Resource {
    fn from(mcp_resource: McpResource) -> Self {
        Self {
            uri: mcp_resource.uri,
            name: mcp_resource.name,
            description: mcp_resource.description,
            mime_type: mcp_resource.mime_type.unwrap_or_default(),
            server_id: String::new(), // Will be set when added to manager
        }
    }
}

/// Resource content with metadata
#[derive(Debug, Clone)]
pub struct ResourceContent {
    /// Resource URI
    pub uri: String,
    /// Content blocks
    pub contents: Vec<McpContent>,
    /// Server that provided the resource
    pub server_id: String,
    /// Fetch time
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

/// Resource manager for handling MCP resources
pub struct ResourceManager {
    /// All discovered resources by URI
    resources: Arc<RwLock<HashMap<String, Resource>>>,
    /// Mapping of `server_id` to resource URIs
    server_resources: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl ResourceManager {
    pub fn new() -> Self {
        Self {
            resources: Arc::new(RwLock::new(HashMap::new())),
            server_resources: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Discover resources from a server
    pub async fn discover_resources(&mut self, server: &McpServer) -> McpResult<Vec<Resource>> {
        let server_id = server.id().to_string();
        info!("Discovering resources from server '{}'", server_id);

        let client_lock = server.client();
        let client = client_lock.read().await;
        let mcp_resources = client
            .list_resources(&server_id)
            .await
            .map_err(|e| McpError::ProtocolError(format!("Failed to discover resources: {e}")))?;
        drop(client);

        let mut resources = Vec::new();
        for mcp_resource in mcp_resources {
            let mut resource = Resource::from(mcp_resource);
            resource.server_id = server_id.clone();
            let uri = resource.uri.clone();

            // Check for conflicts
            let mut resources_map = self.resources.write().await;
            if resources_map.contains_key(&uri) {
                warn!("Resource '{}' already registered, skipping duplicate", uri);
                continue;
            }

            resources_map.insert(uri.clone(), resource.clone());
            resources.push(resource.clone());
            drop(resources_map);

            // Update server resources mapping
            let mut server_resources = self.server_resources.write().await;
            server_resources
                .entry(server_id.clone())
                .or_insert_with(Vec::new)
                .push(uri);
        }

        info!(
            "Discovered {} resources from server '{}'",
            resources.len(),
            server_id
        );

        Ok(resources)
    }

    /// Read a resource by URI
    pub async fn read_resource(&self, server: &McpServer, uri: &str) -> McpResult<ResourceContent> {
        debug!("Reading resource '{}'", uri);

        let server_id = server.id().to_string();

        let client_lock = server.client();
        let client = client_lock.read().await;
        let contents = client
            .read_resource(&server_id, uri)
            .await
            .map_err(|e| McpError::ProtocolError(format!("Failed to read resource: {e}")))?;
        drop(client);

        Ok(ResourceContent {
            uri: uri.to_string(),
            contents: contents.contents,
            server_id,
            fetched_at: chrono::Utc::now(),
        })
    }

    /// List all resources
    pub async fn list_resources(&self) -> Vec<Resource> {
        let resources = self.resources.read().await;
        resources.values().cloned().collect()
    }

    /// Get resource by URI
    pub async fn resource(&self, uri: &str) -> Option<Resource> {
        let resources = self.resources.read().await;
        resources.get(uri).cloned()
    }

    /// List resources for a specific server
    pub async fn list_server_resources(&self, server_id: &str) -> Vec<Resource> {
        let server_resources = self.server_resources.read().await;
        let uris = match server_resources.get(server_id) {
            Some(uris) => uris.clone(),
            None => return Vec::new(),
        };
        drop(server_resources);

        let resources = self.resources.read().await;
        uris.iter()
            .filter_map(|uri| resources.get(uri).cloned())
            .collect()
    }

    /// Search resources by name or description
    pub async fn search_resources(&self, query: &str) -> Vec<Resource> {
        let resources = self.resources.read().await;
        let query_lower = query.to_lowercase();

        resources
            .values()
            .filter(|r| {
                r.name.to_lowercase().contains(&query_lower)
                    || r.description.to_lowercase().contains(&query_lower)
                    || r.uri.to_lowercase().contains(&query_lower)
            })
            .cloned()
            .collect()
    }

    /// Remove all resources for a server
    pub async fn remove_server_resources(&mut self, server_id: &str) {
        info!("Removing resources for server '{}'", server_id);

        let uris = {
            let mut server_resources = self.server_resources.write().await;
            server_resources.remove(server_id).unwrap_or_default()
        };

        let mut resources = self.resources.write().await;
        for uri in uris {
            resources.remove(&uri);
        }
    }

    /// Get resource count
    pub async fn resource_count(&self) -> usize {
        let resources = self.resources.read().await;
        resources.len()
    }

    /// Get server count
    pub async fn server_count(&self) -> usize {
        let server_resources = self.server_resources.read().await;
        server_resources.len()
    }

    /// Filter resources by MIME type
    pub async fn filter_by_mime_type(&self, mime_type: &str) -> Vec<Resource> {
        let resources = self.resources.read().await;
        resources
            .values()
            .filter(|r| r.mime_type.contains(mime_type))
            .cloned()
            .collect()
    }
}

impl Default for ResourceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch resource operations
pub struct ResourceBatch {
    manager: Arc<RwLock<ResourceManager>>,
}

impl ResourceBatch {
    pub const fn new(manager: Arc<RwLock<ResourceManager>>) -> Self {
        Self { manager }
    }

    /// Read multiple resources in parallel
    pub async fn read_multiple(
        &self,
        server: &McpServer,
        uris: Vec<String>,
    ) -> Vec<Result<ResourceContent, McpError>> {
        info!("Reading {} resources in parallel", uris.len());

        let manager = Arc::clone(&self.manager);
        let server_clone = server.clone();

        let futures: Vec<_> = uris
            .into_iter()
            .map(|uri| {
                let manager = Arc::clone(&manager);
                let server = server_clone.clone();
                async move {
                    let manager_guard = manager.read().await;
                    manager_guard
                        .read_resource(&server, &uri)
                        .await
                        .map_err(|e| {
                            warn!("Failed to read resource '{}': {}", uri, e);
                            e
                        })
                }
            })
            .collect();

        futures::future::join_all(futures).await
    }

    /// Discover resources from multiple servers in parallel
    pub async fn discover_multiple(
        &self,
        servers: Vec<McpServer>,
    ) -> HashMap<String, Vec<Resource>> {
        info!("Discovering resources from {} servers", servers.len());

        let manager = Arc::clone(&self.manager);

        let futures: Vec<_> = servers
            .into_iter()
            .map(|server| {
                let manager = Arc::clone(&manager);
                async move {
                    let server_id = server.id().to_string();
                    let mut manager_guard = manager.write().await;
                    match manager_guard.discover_resources(&server).await {
                        Ok(resources) => Some((server_id, resources)),
                        Err(e) => {
                            warn!(
                                "Failed to discover resources from server '{}': {}",
                                server_id, e
                            );
                            None
                        }
                    }
                }
            })
            .collect();

        let results = futures::future::join_all(futures).await;
        results.into_iter().flatten().collect()
    }
}

/// Resource template matching utilities
///
/// Matches URIs against URI templates (e.g., "<file://{path>}", "<mcp://files/{filename>}")
pub mod template_matching {
    use crate::types::McpResourceTemplate;

    /// Match a URI against a single URI template
    ///
    /// Returns the captured parameters if the URI matches the template
    pub fn match_uri_template(uri: &str, template: &str) -> Option<Vec<String>> {
        // Convert template to regex pattern
        // First escape special regex characters (but not { and })
        let escaped = regex_escape(template);
        // Replace {param} with (?P<param>[^/]+) for named capture groups
        let pattern = replace_template_params(&escaped);

        // Build the regex
        let regex_pattern = format!("^{pattern}$");
        match regex::Regex::new(&regex_pattern).ok() {
            Some(re) => {
                let captures = re.captures(uri)?;
                // Extract all captured groups in order
                Some(
                    captures
                        .iter()
                        .skip(1) // Skip full match
                        .filter_map(|m| m.map(|m| m.as_str().to_string()))
                        .collect::<Vec<String>>(),
                )
            }
            None => None,
        }
    }

    /// Replace {param} patterns with named capture groups
    fn replace_template_params(template: &str) -> String {
        let mut result = template.to_string();
        // Match {paramName} and replace with (?P<paramName>[^/]+)
        while let Some(start) = result.find('{') {
            if let Some(end) = result[start..].find('}') {
                let param_name = &result[start + 1..start + end];
                let replacement = format!("(?P<{param_name}>[^/]+)");
                result.replace_range(start..=(start + end), &replacement);
            } else {
                break;
            }
        }
        result
    }

    /// Find a matching template for a URI from a list of templates
    pub fn find_matching_template<'a>(
        uri: &str,
        templates: impl IntoIterator<Item = &'a McpResourceTemplate>,
    ) -> Option<&'a McpResourceTemplate> {
        templates
            .into_iter()
            .find(|template| match_uri_template(uri, &template.uri_template).is_some())
    }

    /// Find either an exact resource match or a matching template
    pub fn find_matching_resource_or_template<'a>(
        uri: &str,
        resources: impl IntoIterator<Item = &'a crate::types::McpResource>,
        templates: impl IntoIterator<Item = &'a McpResourceTemplate>,
    ) -> Option<ResourceOrTemplate<'a>> {
        // First try exact resource match
        let exact_match = resources.into_iter().find(|resource| resource.uri == uri);

        if let Some(resource) = exact_match {
            return Some(ResourceOrTemplate::Resource(resource));
        }

        // Try template match
        find_matching_template(uri, templates).map(ResourceOrTemplate::Template)
    }

    /// Escape regex special characters except for { and }
    pub fn regex_escape(s: &str) -> String {
        s.chars()
            .flat_map(|c| {
                match c {
                    '.' => vec!['\\', '.'],
                    '*' => vec!['\\', '*'],
                    '+' => vec!['\\', '+'],
                    '?' => vec!['\\', '?'],
                    '^' => vec!['\\', '^'],
                    '$' => vec!['\\', '$'],
                    '[' => vec!['\\', '['],
                    ']' => vec!['\\', ']'],
                    '(' => vec!['\\', '('],
                    ')' => vec!['\\', ')'],
                    '|' => vec!['\\', '|'],
                    '\\' => vec!['\\', '\\'],
                    // Don't escape { and } - we handle them specially
                    '{' | '}' => vec![c],
                    _ => vec![c],
                }
            })
            .collect()
    }

    /// Result of `find_matching_resource_or_template`
    #[derive(Debug, Clone)]
    pub enum ResourceOrTemplate<'a> {
        Resource(&'a crate::types::McpResource),
        Template(&'a McpResourceTemplate),
    }

    impl ResourceOrTemplate<'_> {
        pub fn name(&self) -> &str {
            match self {
                ResourceOrTemplate::Resource(r) => &r.name,
                ResourceOrTemplate::Template(t) => &t.name,
            }
        }

        pub fn description(&self) -> &str {
            match self {
                ResourceOrTemplate::Resource(r) => &r.description,
                ResourceOrTemplate::Template(t) => &t.description,
            }
        }

        pub fn mime_type(&self) -> Option<&str> {
            match self {
                ResourceOrTemplate::Resource(r) => r.mime_type.as_deref(),
                ResourceOrTemplate::Template(t) => t.mime_type.as_deref(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::template_matching::{
        find_matching_template, match_uri_template, regex_escape,
    };

    #[test]
    fn test_regex_escape() {
        assert_eq!(regex_escape("hello"), "hello");
        assert_eq!(regex_escape("hello.world"), "hello\\.world");
        assert_eq!(regex_escape("file://{path}"), "file://{path}");
    }

    #[test]
    fn test_match_uri_template_simple() {
        // Template: file://{path} - note: {path} matches until next /
        // For file:///etc/hosts we need file://{slash}{path} or use .* pattern
        // Let's test with a simpler case first
        let result = match_uri_template("file://etc/hosts", "file://{host}/{path}");
        assert!(result.is_some());
        let params = result.unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], "etc");
        assert_eq!(params[1], "hosts");
    }

    #[test]
    fn test_match_uri_template_multiple_params() {
        // Template: mcp://{server}/{resource}
        let result = match_uri_template("mcp://files/document.pdf", "mcp://{server}/{resource}");
        assert!(result.is_some());
        let params = result.unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], "files");
        assert_eq!(params[1], "document.pdf");
    }

    #[test]
    fn test_match_uri_template_greedy() {
        // Template with greedy match for rest of path
        let result = match_uri_template("file:///etc/hosts", "file://{rest}");
        // This won't work with [^/]+ since it stops at slashes
        // We'd need a different pattern for greedy matching
        assert!(result.is_none()); // Expected - our current pattern doesn't support greedy
    }

    #[test]
    fn test_match_uri_template_no_match() {
        let result = match_uri_template("http://example.com", "file://{path}");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_matching_template() {
        use crate::types::McpResourceTemplate;

        let templates = vec![
            McpResourceTemplate {
                uri_template: "file://{host}/{path}".to_string(),
                name: "File System".to_string(),
                description: "Access files".to_string(),
                mime_type: None,
                title: None,
                annotations: None,
            },
            McpResourceTemplate {
                uri_template: "mcp://{server}/{resource}".to_string(),
                name: "MCP Generic".to_string(),
                description: "Generic MCP resource".to_string(),
                mime_type: None,
                title: None,
                annotations: None,
            },
        ];

        // Match file template
        let result = find_matching_template("file://etc/hosts", &templates);
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "File System");

        // Match MCP template
        let result = find_matching_template("mcp://files/doc.pdf", &templates);
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "MCP Generic");

        // No match
        let result = find_matching_template("http://example.com", &templates);
        assert!(result.is_none());
    }
    #[tokio::test]
    async fn test_resource_content() {
        let content = ResourceContent {
            uri: "test://resource".to_string(),
            contents: vec![McpContent::Text {
                text: "Hello, World!".to_string(),
            }],
            server_id: "test-server".to_string(),
            fetched_at: chrono::Utc::now(),
        };

        assert_eq!(content.uri, "test://resource");
        assert_eq!(content.server_id, "test-server");
        assert_eq!(content.contents.len(), 1);
    }

    #[test]
    fn test_resource_serialization_roundtrip() {
        let resource = Resource {
            uri: "file:///test.rs".to_string(),
            name: "test.rs".to_string(),
            description: "A Rust file".to_string(),
            mime_type: "text/rust".to_string(),
            server_id: "my-server".to_string(),
        };
        let json = serde_json::to_string(&resource).unwrap();
        let parsed: Resource = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.uri, resource.uri);
        assert_eq!(parsed.name, resource.name);
        assert_eq!(parsed.description, resource.description);
        assert_eq!(parsed.mime_type, resource.mime_type);
        assert_eq!(parsed.server_id, resource.server_id);
    }

    #[test]
    fn test_resource_from_mcp_resource() {
        let mcp = McpResource {
            uri: "file:///x".to_string(),
            name: "x".to_string(),
            description: "desc".to_string(),
            mime_type: Some("text/plain".to_string()),
            title: None,
            annotations: None,
            size: None,
        };
        let resource: Resource = mcp.into();
        assert_eq!(resource.uri, "file:///x");
        assert_eq!(resource.name, "x");
        assert!(resource.server_id.is_empty());
    }

    #[tokio::test]
    async fn test_resource_manager_new() {
        let manager = ResourceManager::new();
        assert_eq!(manager.resource_count().await, 0);
        assert_eq!(manager.server_count().await, 0);
        assert!(manager.list_resources().await.is_empty());
    }

    #[tokio::test]
    async fn test_resource_manager_default() {
        let manager = ResourceManager::default();
        assert_eq!(manager.resource_count().await, 0);
    }

    #[tokio::test]
    async fn test_resource_manager_get_not_found() {
        let manager = ResourceManager::new();
        assert!(manager.resource("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_resource_manager_list_server_resources_empty() {
        let manager = ResourceManager::new();
        let result = manager.list_server_resources("unknown").await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_resource_manager_search_empty() {
        let manager = ResourceManager::new();
        let results = manager.search_resources("test").await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_resource_manager_filter_mime_empty() {
        let manager = ResourceManager::new();
        let results = manager.filter_by_mime_type("text/plain").await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_resource_manager_remove_server_noop() {
        let mut manager = ResourceManager::new();
        // Removing from empty manager should not panic
        manager.remove_server_resources("unknown").await;
        assert_eq!(manager.resource_count().await, 0);
    }

    #[test]
    fn test_regex_escape_special_chars() {
        assert_eq!(regex_escape("a.b"), "a\\.b");
        assert_eq!(regex_escape("a*b"), "a\\*b");
        assert_eq!(regex_escape("a+b"), "a\\+b");
        assert_eq!(regex_escape("a?b"), "a\\?b");
        assert_eq!(regex_escape("a^b"), "a\\^b");
        assert_eq!(regex_escape("a$b"), "a\\$b");
        assert_eq!(regex_escape("a[b]c"), "a\\[b\\]c");
        assert_eq!(regex_escape("a(b)c"), "a\\(b\\)c");
        assert_eq!(regex_escape("a|b"), "a\\|b");
        assert_eq!(regex_escape("a\\b"), "a\\\\b");
    }

    #[test]
    fn test_match_uri_template_empty_params() {
        let result = match_uri_template("file://static", "file://static");
        assert!(result.is_some());
        let params = result.unwrap();
        assert!(params.is_empty());
    }

    #[test]
    fn test_find_matching_template_no_templates() {
        let templates: Vec<McpResourceTemplate> = vec![];
        let result = find_matching_template("any://uri", &templates);
        assert!(result.is_none());
    }

    #[test]
    fn test_find_matching_resource_or_template() {
        use crate::resources::template_matching::find_matching_resource_or_template;
        use crate::resources::template_matching::ResourceOrTemplate;

        let resources = vec![McpResource {
            uri: "file:///exact".to_string(),
            name: "Exact".to_string(),
            description: "Exact match".to_string(),
            mime_type: Some("text/plain".to_string()),
            title: None,
            annotations: None,
            size: None,
        }];
        let templates = vec![McpResourceTemplate {
            uri_template: "mcp://{server}/{resource}".to_string(),
            name: "MCP".to_string(),
            description: "Template".to_string(),
            mime_type: Some("application/json".to_string()),
            title: None,
            annotations: None,
        }];

        // Exact resource match
        let result = find_matching_resource_or_template("file:///exact", &resources, &templates);
        assert!(result.is_some());
        let rot = result.unwrap();
        assert!(matches!(rot, ResourceOrTemplate::Resource(_)));
        assert_eq!(rot.name(), "Exact");
        assert_eq!(rot.description(), "Exact match");
        assert_eq!(rot.mime_type(), Some("text/plain"));

        // Template match
        let result = find_matching_resource_or_template("mcp://files/doc", &resources, &templates);
        assert!(result.is_some());
        let rot = result.unwrap();
        assert!(matches!(rot, ResourceOrTemplate::Template(_)));
        assert_eq!(rot.name(), "MCP");
        assert_eq!(rot.mime_type(), Some("application/json"));

        // No match
        let result = find_matching_resource_or_template("http://nope", &resources, &templates);
        assert!(result.is_none());
    }

    #[test]
    fn test_resource_or_template_mime_type_none() {
        use crate::resources::template_matching::ResourceOrTemplate;

        let template = McpResourceTemplate {
            uri_template: "x://{p}".to_string(),
            name: "X".to_string(),
            description: "D".to_string(),
            mime_type: None,
            title: None,
            annotations: None,
        };
        let rot = ResourceOrTemplate::Template(&template);
        assert!(rot.mime_type().is_none());
    }
}
