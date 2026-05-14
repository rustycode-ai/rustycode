// ============================================================================
// Tool Plugin System — Tower Service/Layer design
// ============================================================================
// Three-tier separation:
//   Tool trait        → what tool authors implement (typed params, metadata)
//   ToolService trait → what middleware wraps (erased, composable)
//   Layer trait       → how middleware is stacked

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

pub struct ToolRequest {
    pub params: Value,
    pub context: ToolContext,
}

pub struct ToolOutput {
    pub text: String,
    pub metadata: Value,
}

impl ToolOutput {
    pub fn text(s: impl Into<String>) -> Self {
        Self { text: s.into(), metadata: Value::Null }
    }

    pub fn with_metadata(mut self, meta: Value) -> Self {
        self.metadata = meta;
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("validation: {0}")]
    Validation(String),
    #[error("permission denied: {0}")]
    Permission(String),
    #[error("cancelled")]
    Cancelled,
    #[error("rate limited, retry after {retry_after:?}")]
    RateLimited { retry_after: Duration },
    #[error("{0}")]
    Execution(String),
}

pub type ToolResult = Result<ToolOutput, ToolError>;

// ---------------------------------------------------------------------------
// ToolMeta — static metadata, middleware never touches this
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ToolMeta {
    pub name: String,
    pub description: String,
    pub parameters_schema: Value,
    pub permissions: Permissions,
}

#[derive(Clone, Debug, Default)]
pub struct Permissions {
    pub read_only: bool,
    pub destructive: bool,
    pub requires_approval: bool,
    pub network_access: bool,
}

// ---------------------------------------------------------------------------
// Extensible context — tools request what they need via typed extraction
// ---------------------------------------------------------------------------

pub struct ToolContext {
    extensions: Extensions,
    cancel: CancellationToken,
}

impl ToolContext {
    pub fn new(extensions: Extensions) -> Self {
        Self { extensions, cancel: CancellationToken::new() }
    }

    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.extensions.get::<T>()
    }

    pub fn cancel_token(&self) -> &CancellationToken {
        &self.cancel
    }
}

pub struct Extensions {
    map: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Extensions {
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }

    pub fn insert<T: Send + Sync + 'static>(&mut self, val: T) -> &mut Self {
        self.map.insert(TypeId::of::<T>(), Box::new(val));
        self
    }

    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.map.get(&TypeId::of::<T>())?.downcast_ref()
    }
}

// Example context capabilities (tools opt-in via get::<T>())
pub struct WorkingDir(pub std::path::PathBuf);
pub struct FileSystem;              // facade for tokio::fs
pub struct HttpClient;              // shared reqwest client
pub struct SandboxConfig {
    pub allow_network: bool,
    pub allow_paths: Vec<std::path::PathBuf>,
    pub max_command_time: Duration,
}

// ---------------------------------------------------------------------------
// Tool trait — what tool AUTHORS implement
// ---------------------------------------------------------------------------

pub trait Tool: Send + Sync + 'static {
    type Params: DeserializeOwned + 'static;

    fn meta() -> ToolMeta;

    fn validate(_params: &Self::Params) -> Result<(), ToolError> {
        Ok(())
    }

    fn invoke(
        params: Self::Params,
        ctx: &ToolContext,
    ) -> impl Future<Output = ToolResult> + Send;
}

// ---------------------------------------------------------------------------
// ToolService trait — Tower-style, what MIDDLEWARE wraps
// ---------------------------------------------------------------------------

pub trait ToolService: Send + Sync {
    fn invoke(
        &self,
        req: ToolRequest,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>>;
}

// Blanket: every Tool automatically becomes a ToolService
// Handles JSON → typed deserialization + validation
impl<T: Tool> ToolService for T {
    fn invoke(
        &self,
        req: ToolRequest,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>> {
        Box::pin(async move {
            let params: T::Params = serde_json::from_value(req.params)
                .map_err(|e| ToolError::Validation(e.to_string()))?;
            T::validate(&params)?;
            T::invoke(params, &req.context).await
        })
    }
}

// ---------------------------------------------------------------------------
// Layer trait — Tower Layer pattern
// ---------------------------------------------------------------------------

pub trait Layer<S> {
    type Service: ToolService;
    fn layer(&self, inner: S) -> Self::Service;
}

// ---------------------------------------------------------------------------
// Built-in layers
// ---------------------------------------------------------------------------

// --- Audit layer ---

pub struct AuditLog {
    tx: tokio::sync::mpsc::UnboundedSender<AuditEntry>,
}

pub struct AuditEntry {
    pub tool: String,
    pub elapsed: Duration,
    pub ok: bool,
}

pub struct AuditLayer {
    log: Arc<AuditLog>,
}

pub struct AuditService<S> {
    inner: S,
    log: Arc<AuditLog>,
}

impl AuditLayer {
    pub fn new(log: Arc<AuditLog>) -> Self {
        Self { log }
    }
}

impl<S: ToolService> Layer<S> for AuditLayer {
    type Service = AuditService<S>;
    fn layer(&self, inner: S) -> AuditService<S> {
        AuditService { inner, log: self.log.clone() }
    }
}

impl<S: ToolService> ToolService for AuditService<S> {
    fn invoke(
        &self,
        req: ToolRequest,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>> {
        Box::pin(async move {
            let start = std::time::Instant::now();
            let result = self.inner.invoke(req).await;
            let _ = self.log.tx.send(AuditEntry {
                tool: String::new(),
                elapsed: start.elapsed(),
                ok: result.is_ok(),
            });
            result
        })
    }
}

// --- Rate limit layer ---

pub struct RateLimitLayer {
    permits: usize,
    per: Duration,
}

pub struct RateLimitService<S> {
    inner: S,
    semaphore: Arc<Semaphore>,
}

impl RateLimitLayer {
    pub fn new(permits: usize) -> Self {
        Self { permits, per: Duration::from_secs(1) }
    }
}

impl<S: ToolService> Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;
    fn layer(&self, inner: S) -> RateLimitService<S> {
        RateLimitService {
            inner,
            semaphore: Arc::new(Semaphore::new(self.permits)),
        }
    }
}

impl<S: ToolService> ToolService for RateLimitService<S> {
    fn invoke(
        &self,
        req: ToolRequest,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>> {
        Box::pin(async move {
            let _permit = self
                .semaphore
                .acquire()
                .await
                .map_err(|_| ToolError::Execution("semaphore closed".into()))?;
            self.inner.invoke(req).await
        })
    }
}

// --- Sandbox layer ---

pub struct SandboxLayer {
    config: SandboxConfig,
}

pub struct SandboxService<S> {
    inner: S,
    config: Arc<SandboxConfig>,
}

impl SandboxLayer {
    pub fn new(config: SandboxConfig) -> Self {
        Self { config }
    }
}

impl<S: ToolService> Layer<S> for SandboxLayer {
    type Service = SandboxService<S>;
    fn layer(&self, inner: S) -> SandboxService<S> {
        SandboxService {
            inner,
            config: Arc::new(self.config.clone()),
        }
    }
}

impl<S: ToolService> ToolService for SandboxService<S> {
    fn invoke(
        &self,
        mut req: ToolRequest,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>> {
        let config = self.config.clone();
        Box::pin(async move {
            if !config.allow_network {
                // validate params don't request network (example check)
            }
            req.context.extensions.insert(config.clone());
            self.inner.invoke(req).await
        })
    }
}

// --- Retry layer ---

pub struct RetryLayer {
    max_attempts: usize,
    backoff: Duration,
}

pub struct RetryService<S> {
    inner: S,
    max_attempts: usize,
    backoff: Duration,
}

impl RetryLayer {
    pub fn new(max_attempts: usize) -> Self {
        Self { max_attempts, backoff: Duration::from_millis(100) }
    }
}

impl<S: ToolService + Clone> Layer<S> for RetryLayer {
    //           ^^^^^^ Clone needed to re-invoke on retry
    type Service = RetryService<S>;
    fn layer(&self, inner: S) -> RetryService<S> {
        RetryService {
            inner,
            max_attempts: self.max_attempts,
            backoff: self.backoff,
        }
    }
}

impl<S: ToolService + Clone> ToolService for RetryService<S> {
    fn invoke(
        &self,
        req: ToolRequest,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>> {
        Box::pin(async move {
            let mut last_err = ToolError::Execution("no attempts".into());
            for _ in 0..self.max_attempts {
                let req_clone = ToolRequest {
                    params: req.params.clone(),
                    context: ToolContext::new(Extensions::new()),
                };
                match self.inner.invoke(req_clone).await {
                    ok @ Ok(_) => return ok,
                    Err(e) => {
                        last_err = e;
                        tokio::time::sleep(self.backoff).await;
                    }
                }
            }
            Err(last_err)
        })
    }
}

// ---------------------------------------------------------------------------
// Stack helper — fluent layer composition
// ---------------------------------------------------------------------------

pub struct Stack<S>(pub S);

impl<S> Stack<S> {
    pub fn layer<L: Layer<S>>(self, l: L) -> Stack<L::Service> {
        Stack(l.layer(self.0))
    }

    pub fn into_service(self) -> S {
        self.0
    }
}

impl Stack<()> {
    pub fn new() -> Self {
        Stack(())
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

struct RegisteredTool {
    meta: ToolMeta,
    service: Arc<dyn ToolService>,
}

pub struct ToolRegistry {
    entries: HashMap<String, RegisteredTool>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Register a bare Tool — auto-gets ToolService blanket impl
    pub fn register<T: Tool + Default>(&mut self) {
        let meta = T::meta();
        let svc: Arc<dyn ToolService> = Arc::new(T::default());
        self.entries.insert(meta.name.clone(), RegisteredTool { meta, svc });
    }

    /// Register with a pre-composed service (layers already applied)
    pub fn register_service(&mut self, meta: ToolMeta, svc: impl ToolService + 'static) {
        self.entries.insert(meta.name.clone(), RegisteredTool {
            meta,
            service: Arc::new(svc),
        });
    }

    /// Register a dynamic tool (MCP) — same interface, different source
    pub fn register_dynamic(&mut self, meta: ToolMeta, svc: impl ToolService + 'static) {
        self.register_service(meta, svc);
    }

    /// Unregister (MCP server disconnected)
    pub fn unregister(&mut self, name: &str) {
        self.entries.remove(name);
    }

    pub async fn invoke(
        &self,
        name: &str,
        params: Value,
        ctx: ToolContext,
    ) -> ToolResult {
        let entry = self
            .entries
            .get(name)
            .ok_or_else(|| ToolError::Execution(format!("unknown tool: {name}")))?;
        entry.service.invoke(ToolRequest { params, context: ctx }).await
    }

    pub fn list(&self) -> Vec<&ToolMeta> {
        self.entries.values().map(|e| &e.meta).collect()
    }

    pub fn get_meta(&self, name: &str) -> Option<&ToolMeta> {
        self.entries.get(name).map(|e| &e.meta)
    }
}

// ---------------------------------------------------------------------------
// MCP tool adapter — proves dynamic tools use the same interface
// ---------------------------------------------------------------------------

pub struct McpToolService {
    client: Arc<dyn McpClient>,
    tool_name: String,
}

#[async_trait::async_trait]
pub trait McpClient: Send + Sync {
    async fn call_tool(&self, name: &str, params: Value) -> ToolResult;
}

impl ToolService for McpToolService {
    fn invoke(
        &self,
        req: ToolRequest,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>> {
        Box::pin(async move {
            self.client.call_tool(&self.tool_name, req.params).await
        })
    }
}

// ---------------------------------------------------------------------------
// #[tool] macro replacement for define_tool!
// ---------------------------------------------------------------------------

// Before (16 methods):
//   define_tool! { name: "bash", ... fn execute() { ... } }
//
// After — tool authors implement a single async fn:
//
//   pub struct BashTool;
//
//   impl Tool for BashTool {
//       type Params = BashParams;
//       fn meta() -> ToolMeta { ... }
//       async fn invoke(params: BashParams, ctx: &ToolContext) -> ToolResult {
//           // your code here
//       }
//   }
//
// A derive/attribute macro can generate meta() + validate() from struct attrs:
//
//   #[tool(name = "bash", description = "Run shell commands", destructive)]
//   #[derive(Deserialize, schemars::JsonSchema)]
//   struct BashParams {
//       command: String,
//       #[serde(default)]
//       timeout: u64,
//   }
