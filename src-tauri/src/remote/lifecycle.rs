//! Server lifecycle management for remote transcription
//!
//! Handles starting and stopping the HTTP server when sharing is enabled/disabled.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use warp::Filter;

use super::http::{create_routes, ServerContext};
use super::transcription::{RealTranscriptionContext, TranscriptionServerConfig};

/// Handle to a running server, used to stop it
pub struct ServerHandle {
    /// Channel to signal server shutdown
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// The port the server is listening on
    pub port: u16,
}

impl ServerHandle {
    /// Stop the server gracefully
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
            log::info!("Server shutdown signal sent");
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Server lifecycle manager
pub struct RemoteServerManager {
    /// Handle to the currently running server (if any)
    handle: Option<ServerHandle>,
    /// Server configuration
    config: Option<TranscriptionServerConfig>,
}

impl Default for RemoteServerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteServerManager {
    /// Create a new server manager
    pub fn new() -> Self {
        Self {
            handle: None,
            config: None,
        }
    }

    /// Check if the server is currently running
    pub fn is_running(&self) -> bool {
        self.handle.is_some()
    }

    /// Get the port the server is listening on (if running)
    pub fn get_port(&self) -> Option<u16> {
        self.handle.as_ref().map(|h| h.port)
    }

    /// Start the remote transcription server
    ///
    /// # Arguments
    /// * `port` - Port to listen on
    /// * `password` - Optional password for authentication
    /// * `server_name` - Display name for this server
    /// * `model_path` - Path to the currently selected model
    /// * `model_name` - Name of the current model
    pub async fn start(
        &mut self,
        port: u16,
        password: Option<String>,
        server_name: String,
        model_path: PathBuf,
        model_name: String,
    ) -> Result<(), String> {
        // Stop existing server if running
        if self.handle.is_some() {
            self.stop();
        }

        let config = TranscriptionServerConfig {
            server_name: server_name.clone(),
            password,
            model_path,
            model_name,
        };

        self.config = Some(config.clone());

        // Create the transcription context
        let ctx = Arc::new(Mutex::new(RealTranscriptionContext::new(config)));

        // Create routes
        let routes = create_routes(ctx);

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        // Bind to address
        let addr: SocketAddr = ([0, 0, 0, 0], port).into();

        log::info!("Starting remote transcription server on {}", addr);

        // Create the server with graceful shutdown
        let (_, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
            shutdown_rx.await.ok();
            log::info!("Server received shutdown signal");
        });

        // Spawn the server task
        tokio::spawn(async move {
            server.await;
            log::info!("Server task completed");
        });

        // Store the handle
        self.handle = Some(ServerHandle {
            shutdown_tx: Some(shutdown_tx),
            port,
        });

        log::info!(
            "Remote transcription server started on port {} as '{}'",
            port,
            server_name
        );

        Ok(())
    }

    /// Stop the remote transcription server
    pub fn stop(&mut self) {
        if let Some(mut handle) = self.handle.take() {
            handle.stop();
            log::info!("Remote transcription server stopped");
        }
        self.config = None;
    }

    /// Update the model being served (without restarting server)
    pub fn update_model(&mut self, model_path: PathBuf, model_name: String) {
        if let Some(config) = &mut self.config {
            config.model_path = model_path;
            config.model_name = model_name;
            log::info!("Server model updated to: {}", config.model_name);
        }
    }

    /// Get the current server configuration
    pub fn get_config(&self) -> Option<&TranscriptionServerConfig> {
        self.config.as_ref()
    }
}

/// Information about the sharing status
#[derive(Debug, Clone, serde::Serialize)]
pub struct SharingStatus {
    /// Whether sharing is currently enabled
    pub enabled: bool,
    /// Port the server is listening on (if enabled)
    pub port: Option<u16>,
    /// Name of the model being shared (if enabled)
    pub model_name: Option<String>,
    /// Server display name (if enabled)
    pub server_name: Option<String>,
    /// Number of active connections (placeholder for future)
    pub active_connections: u32,
}

impl RemoteServerManager {
    /// Get the current sharing status
    pub fn get_status(&self) -> SharingStatus {
        if let Some(handle) = &self.handle {
            let config = self.config.as_ref();
            SharingStatus {
                enabled: true,
                port: Some(handle.port),
                model_name: config.map(|c| c.model_name.clone()),
                server_name: config.map(|c| c.server_name.clone()),
                active_connections: 0, // TODO: track actual connections
            }
        } else {
            SharingStatus {
                enabled: false,
                port: None,
                model_name: None,
                server_name: None,
                active_connections: 0,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_manager_new() {
        let manager = RemoteServerManager::new();
        assert!(!manager.is_running());
        assert!(manager.get_port().is_none());
    }

    #[test]
    fn test_sharing_status_disabled() {
        let manager = RemoteServerManager::new();
        let status = manager.get_status();

        assert!(!status.enabled);
        assert!(status.port.is_none());
        assert!(status.model_name.is_none());
        assert!(status.server_name.is_none());
        assert_eq!(status.active_connections, 0);
    }

    #[tokio::test]
    async fn test_server_start_stop() {
        let mut manager = RemoteServerManager::new();

        // Start server
        let result = manager
            .start(
                47843, // Use non-default port for test
                None,
                "Test Server".to_string(),
                PathBuf::from("/fake/model.bin"),
                "test-model".to_string(),
            )
            .await;

        assert!(result.is_ok());
        assert!(manager.is_running());
        assert_eq!(manager.get_port(), Some(47843));

        let status = manager.get_status();
        assert!(status.enabled);
        assert_eq!(status.port, Some(47843));
        assert_eq!(status.model_name, Some("test-model".to_string()));
        assert_eq!(status.server_name, Some("Test Server".to_string()));

        // Stop server
        manager.stop();
        assert!(!manager.is_running());
        assert!(manager.get_port().is_none());

        let status = manager.get_status();
        assert!(!status.enabled);
    }

    #[tokio::test]
    async fn test_server_restart() {
        let mut manager = RemoteServerManager::new();

        // Start first server
        manager
            .start(
                47844,
                None,
                "Server 1".to_string(),
                PathBuf::from("/model1.bin"),
                "model1".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(manager.get_status().model_name, Some("model1".to_string()));

        // Start second server (should stop first)
        manager
            .start(
                47845,
                Some("password".to_string()),
                "Server 2".to_string(),
                PathBuf::from("/model2.bin"),
                "model2".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(manager.get_port(), Some(47845));
        assert_eq!(manager.get_status().model_name, Some("model2".to_string()));
        assert_eq!(
            manager.get_status().server_name,
            Some("Server 2".to_string())
        );

        manager.stop();
    }
}
