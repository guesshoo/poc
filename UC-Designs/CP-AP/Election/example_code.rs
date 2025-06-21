// Not working
use std::sync::Arc;
use std::time::Duration;

use async_raft::{
    Config, NodeId, Raft, RaftNetwork, RaftStorage, AppData, AppDataResponse,
};
use async_raft::raft::{ClientWriteRequest, Entry};
use async_raft::storage::MemStore;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::time::sleep;

/// A dummy application request; here we just log the data.
#[derive(Clone, Debug)]
struct ClientRequest {
    pub data: String,
}
impl AppData for ClientRequest {}

/// The response type for our dummy app.
#[derive(Clone, Debug)]
struct ClientResponse {
    pub success: bool,
}
impl AppDataResponse for ClientResponse {}

/// In-memory “network” that simply routes Raft RPCs via Tokio channels.
struct MemNetwork {
    peers: RwLock<std::collections::HashMap<NodeId, mpsc::UnboundedSender<async_raft::raft::RaftRPC<ClientRequest>>>>,
}

impl MemNetwork {
    fn new() -> Self {
        Self { peers: RwLock::new(Default::default()) }
    }

    /// Register a node’s RPC receiver under its ID
    async fn register(&self, id: NodeId, tx: mpsc::UnboundedSender<async_raft::raft::RaftRPC<ClientRequest>>) {
        self.peers.write().await.insert(id, tx);
    }
}

#[async_trait::async_trait]
impl RaftNetwork<ClientRequest> for MemNetwork {
    async fn send_append_entries(
        &self,
        target: NodeId,
        rpc: async_raft::raft::AppendEntriesRequest<ClientRequest>,
    ) -> Result<async_raft::raft::AppendEntriesResponse, async_raft::error::RPCError> {
        // wrap the request in the RaftRPC enum and send it
        let (tx, rx) = oneshot::channel();
        let rpc = async_raft::raft::RaftRPC::AppendEntries { rpc, tx };
        self.peers.read().await[&target].send(rpc).unwrap();
        rx.await.map_err(|_| async_raft::error::RPCError::NetworkFailure)?
    }

    async fn send_install_snapshot(
        &self,
        _target: NodeId,
        _rpc: async_raft::raft::InstallSnapshotRequest,
    ) -> Result<async_raft::raft::InstallSnapshotResponse, async_raft::error::RPCError> {
        unimplemented!("not needed for this simple demo")
    }

    async fn send_vote(
        &self,
        target: NodeId,
        rpc: async_raft::raft::RequestVoteRequest,
    ) -> Result<async_raft::raft::RequestVoteResponse, async_raft::error::RPCError> {
        let (tx, rx) = oneshot::channel();
        let rpc = async_raft::raft::RaftRPC::RequestVote { rpc, tx };
        self.peers.read().await[&target].send(rpc).unwrap();
        rx.await.map_err(|_| async_raft::error::RPCError::NetworkFailure)?
    }
}

/// Build and spawn a single Raft node with in-memory transport & storage.
async fn spawn_node(
    id: NodeId,
    network: Arc<MemNetwork>,
    config: Arc<Config>,
) -> Arc<Raft<ClientRequest, ClientResponse, MemNetwork, MemStore<ClientRequest>>> {
    // in-memory storage:
    let store = MemStore::new(config.clone());
    let raft = Raft::new(id, config.clone(), network.clone(), store.clone());

    // create an mpsc to receive incoming RaftRPCs:
    let (tx, mut rx) = mpsc::unbounded_channel();
    network.register(id, tx).await;

    // pump incoming messages into the Raft instance:
    let raft_clone = raft.clone();
    tokio::spawn(async move {
        while let Some(rpc) = rx.recv().await {
            use async_raft::raft::RaftRPC::*;
            match rpc {
                AppendEntries { rpc, tx }   => { let res = raft_clone.append_entries(rpc).await; let _ = tx.send(res); }
                RequestVote { rpc, tx }     => { let res = raft_clone.vote(rpc).await;           let _ = tx.send(res); }
                _ => {}
            }
        }
    });

    raft
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1) Raft configuration:
    let config = Arc::new(
        Config::build("demo_cluster".into())
            .election_timeout_min(150)   // ms
            .election_timeout_max(300)   // ms
            .validate()?
    );

    // 2) Shared in-memory network:
    let network = Arc::new(MemNetwork::new());

    // 3) Spawn 3 nodes:
    let mut nodes = Vec::new();
    for id in 1..=3 {
        let node = spawn_node(id, network.clone(), config.clone()).await;
        nodes.push(node);
    }

    // 4) Add all peers to each node’s membership:
    for node in &nodes {
        node.change_membership(vec![1,2,3], false).await?;
    }

    // 5) Wait a bit (longer than upper election timeout):
    sleep(Duration::from_millis(500)).await;

    // 6) Inspect who’s leader:
    let mut leaders = Vec::new();
    for node in &nodes {
        if node.metrics().borrow().state == async_raft::State::Leader {
            leaders.push(node.id());
        }
    }
    println!("Leader(s): {:?}", leaders);

    Ok(())
}
