use super::storage::{ArborConfig, ArborStorage};
use super::types::{ArborEvent, Handle, NodeId, TreeId, TreeSkeleton};
use async_stream::stream;
use futures::Stream;
use hub_macro::hub_methods;
use serde_json::Value;
use std::sync::Arc;

/// Arbor plugin - manages conversation trees
#[derive(Clone)]
pub struct Arbor {
    storage: Arc<ArborStorage>,
}

impl Arbor {
    /// Create a new Arbor activation with its own storage
    pub async fn new(config: ArborConfig) -> Result<Self, String> {
        let storage = ArborStorage::new(config)
            .await
            .map_err(|e| format!("Failed to initialize Arbor storage: {}", e.message))?;

        Ok(Self {
            storage: Arc::new(storage),
        })
    }

    /// Create an Arbor activation with a shared storage instance
    pub fn with_storage(storage: Arc<ArborStorage>) -> Self {
        Self { storage }
    }

    /// Get the underlying storage (for sharing with other activations)
    pub fn storage(&self) -> Arc<ArborStorage> {
        self.storage.clone()
    }
}

#[hub_methods(
    namespace = "arbor",
    version = "1.0.0",
    description = "Manage conversation trees with context tracking"
)]
impl Arbor {
    /// Create a new conversation tree
    #[hub_macro::hub_method(params(
        metadata = "Optional tree-level metadata (name, description, etc.)",
        owner_id = "Owner identifier (default: 'system')"
    ))]
    async fn tree_create(
        &self,
        metadata: Option<Value>,
        owner_id: String,
    ) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.tree_create(metadata, &owner_id).await {
                Ok(tree_id) => yield ArborEvent::TreeCreated { tree_id },
                Err(e) => {
                    eprintln!("Error creating tree: {}", e.message);
                    yield ArborEvent::TreeCreated { tree_id: TreeId::nil() };
                }
            }
        }
    }

    /// Retrieve a complete tree with all nodes
    #[hub_macro::hub_method(params(tree_id = "UUID of the tree to retrieve"))]
    async fn tree_get(&self, tree_id: TreeId) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.tree_get(&tree_id).await {
                Ok(tree) => yield ArborEvent::TreeData { tree },
                Err(e) => {
                    eprintln!("Error getting tree: {}", e.message);
                    yield ArborEvent::TreeList { tree_ids: vec![] };
                }
            }
        }
    }

    /// Get lightweight tree structure without node data
    #[hub_macro::hub_method(params(tree_id = "UUID of the tree to retrieve"))]
    async fn tree_get_skeleton(
        &self,
        tree_id: TreeId,
    ) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.tree_get(&tree_id).await {
                Ok(tree) => yield ArborEvent::TreeSkeleton { skeleton: TreeSkeleton::from(&tree) },
                Err(e) => {
                    eprintln!("Error getting tree skeleton: {}", e.message);
                    yield ArborEvent::TreeList { tree_ids: vec![] };
                }
            }
        }
    }

    /// List all active trees
    #[hub_macro::hub_method]
    async fn tree_list(&self) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.tree_list(false).await {
                Ok(tree_ids) => yield ArborEvent::TreeList { tree_ids },
                Err(e) => {
                    eprintln!("Error listing trees: {}", e.message);
                    yield ArborEvent::TreeList { tree_ids: vec![] };
                }
            }
        }
    }

    /// Update tree metadata
    #[hub_macro::hub_method(params(
        tree_id = "UUID of the tree to update",
        metadata = "New metadata to set"
    ))]
    async fn tree_update_metadata(
        &self,
        tree_id: TreeId,
        metadata: Value,
    ) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.tree_update_metadata(&tree_id, metadata).await {
                Ok(_) => yield ArborEvent::TreeUpdated { tree_id },
                Err(e) => {
                    eprintln!("Error updating tree metadata: {}", e.message);
                    yield ArborEvent::TreeList { tree_ids: vec![] };
                }
            }
        }
    }

    /// Claim ownership of a tree (increment reference count)
    #[hub_macro::hub_method(params(
        tree_id = "UUID of the tree to claim",
        owner_id = "Owner identifier",
        count = "Number of references to add (default: 1)"
    ))]
    async fn tree_claim(
        &self,
        tree_id: TreeId,
        owner_id: String,
        count: i64,
    ) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.tree_claim(&tree_id, &owner_id, count).await {
                Ok(new_count) => yield ArborEvent::TreeClaimed { tree_id, owner_id, new_count },
                Err(e) => {
                    eprintln!("Error claiming tree: {}", e.message);
                    yield ArborEvent::TreeList { tree_ids: vec![] };
                }
            }
        }
    }

    /// Release ownership of a tree (decrement reference count)
    #[hub_macro::hub_method(params(
        tree_id = "UUID of the tree to release",
        owner_id = "Owner identifier",
        count = "Number of references to remove (default: 1)"
    ))]
    async fn tree_release(
        &self,
        tree_id: TreeId,
        owner_id: String,
        count: i64,
    ) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.tree_release(&tree_id, &owner_id, count).await {
                Ok(new_count) => yield ArborEvent::TreeReleased { tree_id, owner_id, new_count },
                Err(e) => {
                    eprintln!("Error releasing tree: {}", e.message);
                    yield ArborEvent::TreeList { tree_ids: vec![] };
                }
            }
        }
    }

    /// List trees scheduled for deletion
    #[hub_macro::hub_method]
    async fn tree_list_scheduled(&self) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.tree_list(true).await {
                Ok(tree_ids) => yield ArborEvent::TreesScheduled { tree_ids },
                Err(e) => {
                    eprintln!("Error listing scheduled trees: {}", e.message);
                    yield ArborEvent::TreesScheduled { tree_ids: vec![] };
                }
            }
        }
    }

    /// List archived trees
    #[hub_macro::hub_method]
    async fn tree_list_archived(&self) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.tree_list(true).await {
                Ok(tree_ids) => yield ArborEvent::TreesArchived { tree_ids },
                Err(e) => {
                    eprintln!("Error listing archived trees: {}", e.message);
                    yield ArborEvent::TreesArchived { tree_ids: vec![] };
                }
            }
        }
    }

    /// Create a text node in a tree
    #[hub_macro::hub_method(params(
        tree_id = "UUID of the tree",
        parent = "Parent node ID (None for root-level)",
        content = "Text content for the node",
        metadata = "Optional node metadata"
    ))]
    async fn node_create_text(
        &self,
        tree_id: TreeId,
        parent: Option<NodeId>,
        content: String,
        metadata: Option<Value>,
    ) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.node_create_text(&tree_id, parent, content, metadata).await {
                Ok(node_id) => yield ArborEvent::NodeCreated { tree_id, node_id, parent },
                Err(e) => {
                    eprintln!("Error creating text node: {}", e.message);
                    yield ArborEvent::TreeList { tree_ids: vec![] };
                }
            }
        }
    }

    /// Create an external node in a tree
    #[hub_macro::hub_method(params(
        tree_id = "UUID of the tree",
        parent = "Parent node ID (None for root-level)",
        handle = "Handle to external data",
        metadata = "Optional node metadata"
    ))]
    async fn node_create_external(
        &self,
        tree_id: TreeId,
        parent: Option<NodeId>,
        handle: Handle,
        metadata: Option<Value>,
    ) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.node_create_external(&tree_id, parent, handle, metadata).await {
                Ok(node_id) => yield ArborEvent::NodeCreated { tree_id, node_id, parent },
                Err(e) => {
                    eprintln!("Error creating external node: {}", e.message);
                    yield ArborEvent::TreeList { tree_ids: vec![] };
                }
            }
        }
    }

    /// Get a node by ID
    #[hub_macro::hub_method(params(
        tree_id = "UUID of the tree",
        node_id = "UUID of the node"
    ))]
    async fn node_get(
        &self,
        tree_id: TreeId,
        node_id: NodeId,
    ) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.node_get(&tree_id, &node_id).await {
                Ok(node) => yield ArborEvent::NodeData { tree_id, node },
                Err(e) => {
                    eprintln!("Error getting node: {}", e.message);
                    yield ArborEvent::TreeList { tree_ids: vec![] };
                }
            }
        }
    }

    /// Get the children of a node
    #[hub_macro::hub_method(params(
        tree_id = "UUID of the tree",
        node_id = "UUID of the node"
    ))]
    async fn node_get_children(
        &self,
        tree_id: TreeId,
        node_id: NodeId,
    ) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.node_get_children(&tree_id, &node_id).await {
                Ok(children) => yield ArborEvent::NodeChildren { tree_id, node_id, children },
                Err(e) => {
                    eprintln!("Error getting node children: {}", e.message);
                    yield ArborEvent::TreeList { tree_ids: vec![] };
                }
            }
        }
    }

    /// Get the parent of a node
    #[hub_macro::hub_method(params(
        tree_id = "UUID of the tree",
        node_id = "UUID of the node"
    ))]
    async fn node_get_parent(
        &self,
        tree_id: TreeId,
        node_id: NodeId,
    ) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.node_get_parent(&tree_id, &node_id).await {
                Ok(parent) => yield ArborEvent::NodeParent { tree_id, node_id, parent },
                Err(e) => {
                    eprintln!("Error getting node parent: {}", e.message);
                    yield ArborEvent::TreeList { tree_ids: vec![] };
                }
            }
        }
    }

    /// Get the path from root to a node
    #[hub_macro::hub_method(params(
        tree_id = "UUID of the tree",
        node_id = "UUID of the node"
    ))]
    async fn node_get_path(
        &self,
        tree_id: TreeId,
        node_id: NodeId,
    ) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.node_get_path(&tree_id, &node_id).await {
                Ok(path) => yield ArborEvent::ContextPath { tree_id, path },
                Err(e) => {
                    eprintln!("Error getting node path: {}", e.message);
                    yield ArborEvent::TreeList { tree_ids: vec![] };
                }
            }
        }
    }

    /// List all leaf nodes in a tree
    #[hub_macro::hub_method(params(tree_id = "UUID of the tree"))]
    async fn context_list_leaves(
        &self,
        tree_id: TreeId,
    ) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.context_list_leaves(&tree_id).await {
                Ok(leaves) => yield ArborEvent::ContextLeaves { tree_id, leaves },
                Err(e) => {
                    eprintln!("Error listing leaf nodes: {}", e.message);
                    yield ArborEvent::TreeList { tree_ids: vec![] };
                }
            }
        }
    }

    /// Get the full path data from root to a node
    #[hub_macro::hub_method(params(
        tree_id = "UUID of the tree",
        node_id = "UUID of the target node"
    ))]
    async fn context_get_path(
        &self,
        tree_id: TreeId,
        node_id: NodeId,
    ) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.context_get_path(&tree_id, &node_id).await {
                Ok(nodes) => yield ArborEvent::ContextPathData { tree_id, nodes },
                Err(e) => {
                    eprintln!("Error getting context path: {}", e.message);
                    yield ArborEvent::TreeList { tree_ids: vec![] };
                }
            }
        }
    }

    /// Get all external handles in the path to a node
    #[hub_macro::hub_method(params(
        tree_id = "UUID of the tree",
        node_id = "UUID of the target node"
    ))]
    async fn context_get_handles(
        &self,
        tree_id: TreeId,
        node_id: NodeId,
    ) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.context_get_handles(&tree_id, &node_id).await {
                Ok(handles) => yield ArborEvent::ContextHandles { tree_id, handles },
                Err(e) => {
                    eprintln!("Error getting context handles: {}", e.message);
                    yield ArborEvent::TreeList { tree_ids: vec![] };
                }
            }
        }
    }

    /// Render tree as text visualization
    #[hub_macro::hub_method(params(tree_id = "UUID of the tree to render"))]
    async fn tree_render(
        &self,
        tree_id: TreeId,
    ) -> impl Stream<Item = ArborEvent> + Send + 'static {
        let storage = self.storage.clone();
        stream! {
            match storage.tree_get(&tree_id).await {
                Ok(tree) => yield ArborEvent::TreeRender { tree_id, render: tree.render() },
                Err(e) => {
                    eprintln!("Error rendering tree: {}", e.message);
                    yield ArborEvent::TreeRender { tree_id, render: format!("Error: {}", e.message) };
                }
            }
        }
    }
}
