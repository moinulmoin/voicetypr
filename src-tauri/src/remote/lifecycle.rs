//! Server lifecycle management for remote transcription
//!
//! Handles starting and stopping the HTTP server when sharing is enabled/disabled.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::{oneshot, Mutex};
use warp::Filter;

use super::http::{create_routes, ServerContext};
use super::transcription::{RealTranscriptionContext, SharedServerState, TranscriptionServerConfig};

/// Handle to a running server, used to stop it
pub struct ServerHandle {
    /// Channels to signal server shutdown (one per bound IP)
    shutdown_txs: Vec<oneshot::Sender<()>>,
    /// The port the server is listening on
    pub port: u16,
    /// The IPs the server is bound to
    pub bound_ips: Vec<IpAddr>,
}

impl ServerHandle {
    /// Stop the server gracefully
    pub fn stop(&mut self) {
        for tx in self.shutdown_txs.drain(..) {
            let _ = tx.send(());
        }
        log::info!("[Remote Server] Shutdown signal sent for port {}", self.port);
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
    /// Shared state for dynamic model updates (only valid while server is running)
    shared_state: Option<SharedServerState>,
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
            shared_state: None,
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
    /// * `engine` - Transcription engine (whisper, parakeet, etc.)
    /// * `app_handle` - Optional AppHandle for Parakeet support
    pub async fn start(
        &mut self,
        port: u16,
        password: Option<String>,
        server_name: String,
        model_path: PathBuf,
        model_name: String,
        engine: String,
        app_handle: Option<AppHandle>,
    ) -> Result<(), String> {
        // Stop existing server if running
        if self.handle.is_some() {
            self.stop();
        }

        let config = TranscriptionServerConfig {
            server_name: server_name.clone(),
            password: password.clone(),
            model_path: model_path.clone(),
            model_name: model_name.clone(),
        };

        self.config = Some(config.clone());

        // Create shared state for dynamic model updates
        let shared_state = SharedServerState::new(model_name, model_path, engine.clone());
        self.shared_state = Some(shared_state.clone());

        // Create the transcription context with shared state and app handle
        // App handle is needed for Parakeet engine support
        let ctx = Arc::new(Mutex::new(RealTranscriptionContext::new_with_shared_state(
            server_name.clone(),
            password,
            shared_state,
            app_handle,
        )));

        // Get all local IPs to bind to
        // On Intel Macs, binding to 0.0.0.0 doesn't work properly for non-localhost connections,
        // so we bind to each specific IP address instead
        let mut bind_ips: Vec<IpAddr> = Vec::new();

        // Always include localhost
        bind_ips.push(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));

        // Add all network interface IPs (only IPv4 for now - IPv6 link-local addresses cause binding issues)
        if let Ok(interfaces) = local_ip_address::list_afinet_netifas() {
            for (name, ip) in interfaces {
                // Skip loopback and IPv6 addresses (IPv6 link-local addresses like fe80:: can't be bound without scope ID)
                if !ip.is_loopback() && ip.is_ipv4() {
                    log::info!("[Remote Server] Found interface {}: {}", name, ip);
                    bind_ips.push(ip);
                }
            }
        }

        log::info!(
            "[Remote Server] Starting server on {} IPs as '{}': {:?}",
            bind_ips.len(), server_name, bind_ips
        );

        let mut shutdown_txs = Vec::new();
        let mut bound_ips = Vec::new();

        for ip in bind_ips {
            let addr: SocketAddr = SocketAddr::new(ip, port);

            // Clone routes for each server instance
            let routes = create_routes(ctx.clone());

            // Create shutdown channel for this instance
            let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
            shutdown_txs.push(shutdown_tx);

            let ip_str = ip.to_string();

            // Create the server with graceful shutdown
            let (_, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async move {
                shutdown_rx.await.ok();
                log::info!("[Remote Server] Received shutdown signal for {}", ip_str);
            });

            // Spawn the server task
            let ip_for_log = ip;
            tokio::spawn(async move {
                server.await;
                log::info!("[Remote Server] Server task completed for {}", ip_for_log);
            });

            bound_ips.push(ip);
            log::info!("[Remote Server] Bound to {}", addr);
        }

        // Store the handle
        self.handle = Some(ServerHandle {
            shutdown_txs,
            port,
            bound_ips,
        });

        log::info!(
            "[Remote Server] Server STARTED on port {} as '{}' with model '{}'",
            port,
            server_name,
            self.config.as_ref().map(|c| c.model_name.as_str()).unwrap_or("unknown")
        );

        Ok(())
    }

    /// Stop the remote transcription server
    pub fn stop(&mut self) {
        if let Some(mut handle) = self.handle.take() {
            let port = handle.port;
            handle.stop();
            log::info!("[Remote Server] Server STOPPED (was on port {})", port);
        }
        self.config = None;
        self.shared_state = None;
    }

    /// Update the model being served (without restarting server)
    ///
    /// This updates the shared state that the running server reads from,
    /// so the change takes effect immediately for new requests.
    pub fn update_model(&mut self, model_path: PathBuf, model_name: String, engine: String) {
        // Update config for tracking
        if let Some(config) = &mut self.config {
            config.model_path = model_path.clone();
            config.model_name = model_name.clone();
        }

        // Update shared state - this is what the running server actually reads
        if let Some(shared_state) = &self.shared_state {
            shared_state.update_model(model_name.clone(), model_path, engine.clone());
            log::info!("[Remote Server] Model dynamically updated to '{}' (engine: {})", model_name, engine);
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
    /// The password for authentication (if set)
    pub password: Option<String>,
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
                password: config.and_then(|c| c.password.clone()),
            }
        } else {
            SharingStatus {
                enabled: false,
                port: None,
                model_name: None,
                server_name: None,
                active_connections: 0,
                password: None,
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

        // Start server (no app handle needed for whisper-only test)
        let result = manager
            .start(
                47843, // Use non-default port for test
                None,
                "Test Server".to_string(),
                PathBuf::from("/fake/model.bin"),
                "test-model".to_string(),
                "whisper".to_string(),
                None,
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
                "whisper".to_string(),
                None,
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
                "whisper".to_string(),
                None,
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
