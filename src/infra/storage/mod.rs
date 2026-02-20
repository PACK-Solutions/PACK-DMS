use async_trait::async_trait;
use bytes::Bytes;
use std::path::PathBuf;
use tokio::fs;

#[async_trait]
pub trait BlobStore: Send + Sync {
    async fn put(&self, key: &str, bytes: Bytes) -> anyhow::Result<()>;
    async fn get(&self, key: &str) -> anyhow::Result<Bytes>;
}

pub struct FileBlobStore {
    root: PathBuf,
}

impl FileBlobStore {
    pub async fn new(root: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let root = root.into();
        if !root.exists() {
            fs::create_dir_all(&root).await?;
        }
        Ok(Self { root })
    }
}

#[async_trait]
impl BlobStore for FileBlobStore {
    async fn put(&self, key: &str, bytes: Bytes) -> anyhow::Result<()> {
        let path = self.root.join(key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(path, bytes).await?;
        Ok(())
    }

    async fn get(&self, key: &str) -> anyhow::Result<Bytes> {
        let path = self.root.join(key);
        let data = fs::read(path).await?;
        Ok(Bytes::from(data))
    }
}

pub struct MemoryBlobStore {
    storage: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, Bytes>>>,
}

impl Default for MemoryBlobStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryBlobStore {
    pub fn new() -> Self {
        Self {
            storage: std::sync::Arc::new(
                tokio::sync::RwLock::new(std::collections::HashMap::new()),
            ),
        }
    }
}

#[async_trait]
impl BlobStore for MemoryBlobStore {
    async fn put(&self, key: &str, bytes: Bytes) -> anyhow::Result<()> {
        let mut storage = self.storage.write().await;
        storage.insert(key.to_string(), bytes);
        Ok(())
    }

    async fn get(&self, key: &str) -> anyhow::Result<Bytes> {
        let storage = self.storage.read().await;
        storage
            .get(key)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("File not found: {}", key))
    }
}
