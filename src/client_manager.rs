use std::{
    collections::HashMap,
    net::SocketAddr,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tokio::sync::{Mutex, watch};

use crate::session::SessionRole;

#[derive(Debug, Clone, Serialize)]
pub struct WebClientInfo {
    pub id: String,
    pub ip: String,
    pub connected_at_ms: u64,
    pub roles: Vec<&'static str>,
}

#[derive(Debug, Default)]
pub struct ClientManager {
    clients: Mutex<HashMap<String, ConnectedClient>>,
}

#[derive(Debug)]
struct ConnectedClient {
    ip: String,
    connected_at_ms: u64,
    roles: HashMap<SessionRole, usize>,
    close_tx: watch::Sender<bool>,
}

#[derive(Debug)]
pub struct ClientSocketLease {
    manager: std::sync::Arc<ClientManager>,
    client_id: String,
    role: SessionRole,
    pub close_rx: watch::Receiver<bool>,
}

impl ClientManager {
    pub async fn register(
        self: &std::sync::Arc<Self>,
        client_id: String,
        peer_addr: SocketAddr,
        role: SessionRole,
    ) -> ClientSocketLease {
        let mut clients = self.clients.lock().await;
        let client = clients.entry(client_id.clone()).or_insert_with(|| {
            let (close_tx, _) = watch::channel(false);
            ConnectedClient {
                ip: peer_addr.ip().to_string(),
                connected_at_ms: now_ms(),
                roles: HashMap::new(),
                close_tx,
            }
        });
        client.ip = peer_addr.ip().to_string();
        *client.roles.entry(role).or_insert(0) += 1;
        ClientSocketLease {
            manager: std::sync::Arc::clone(self),
            client_id,
            role,
            close_rx: client.close_tx.subscribe(),
        }
    }

    pub async fn count(&self) -> usize {
        self.clients.lock().await.len()
    }

    pub async fn list(&self) -> Vec<WebClientInfo> {
        let clients = self.clients.lock().await;
        let mut list = clients
            .iter()
            .map(|(id, client)| WebClientInfo {
                id: id.clone(),
                ip: client.ip.clone(),
                connected_at_ms: client.connected_at_ms,
                roles: client
                    .roles
                    .keys()
                    .copied()
                    .map(role_name)
                    .collect::<Vec<_>>(),
            })
            .collect::<Vec<_>>();
        list.sort_by(|a, b| a.connected_at_ms.cmp(&b.connected_at_ms));
        list
    }

    pub async fn close_client(&self, client_id: &str) -> bool {
        let clients = self.clients.lock().await;
        clients
            .get(client_id)
            .map(|client| client.close_tx.send(true).is_ok())
            .unwrap_or(false)
    }

    pub async fn close_other_clients(&self, current_client_id: &str) -> usize {
        let clients = self.clients.lock().await;
        clients
            .iter()
            .filter(|(client_id, _)| client_id.as_str() != current_client_id)
            .filter(|(_, client)| client.close_tx.send(true).is_ok())
            .count()
    }

    async fn unregister(&self, client_id: &str, role: SessionRole) {
        let mut clients = self.clients.lock().await;
        let Some(client) = clients.get_mut(client_id) else {
            return;
        };
        if let Some(count) = client.roles.get_mut(&role) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                client.roles.remove(&role);
            }
        }
        if client.roles.is_empty() {
            clients.remove(client_id);
        }
    }
}

impl Drop for ClientSocketLease {
    fn drop(&mut self) {
        let manager = std::sync::Arc::clone(&self.manager);
        let client_id = self.client_id.clone();
        let role = self.role;
        tokio::spawn(async move {
            manager.unregister(&client_id, role).await;
        });
    }
}

fn role_name(role: SessionRole) -> &'static str {
    match role {
        SessionRole::All => "all",
        SessionRole::Control => "control",
        SessionRole::Video => "video",
        SessionRole::Audio => "audio",
        SessionRole::Mic => "mic",
        SessionRole::Input => "input",
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
