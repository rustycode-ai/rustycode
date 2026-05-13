use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use anyhow::Result;
use rustycode_runtime::AsyncRuntime;
use rustycode_server::{AppServer, ClientId};
use rustycode_server_client::InProcessClient;
use tokio::sync::mpsc;

pub struct ServiceManager {
    pub runtime: Arc<AsyncRuntime>,
    pub app_server: Arc<Mutex<AppServer>>,
}

impl ServiceManager {
    pub fn new(cwd: PathBuf, runtime: AsyncRuntime) -> Result<Self> {
        let (inbound_tx, inbound_rx) = mpsc::channel(1024);
        let runtime = Arc::new(runtime);
        
        let app_server = AppServer::new(Arc::clone(&runtime), inbound_rx);
        let app_server_arc = Arc::new(Mutex::new(app_server));
        
        Ok(Self {
            runtime,
            app_server: app_server_arc,
        })
    }
}
