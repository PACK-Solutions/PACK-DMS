use async_trait::async_trait;
use bytes::Bytes;
use std::path::PathBuf;
use tokio::fs;

#[async_trait]
pub trait BlobStore: Send + Sync {
    async fn put(&self, key: &str, bytes: Bytes, content_type: Option<&str>) -> anyhow::Result<()>;
    async fn get(&self, key: &str) -> anyhow::Result<Bytes>;
    async fn delete(&self, key: &str) -> anyhow::Result<()>;
    async fn exists(&self, key: &str) -> anyhow::Result<bool>;
    /// Returns the size of the object in bytes, or None if not found.
    async fn head(&self, key: &str) -> anyhow::Result<Option<i64>>;
}

// ---------------------------------------------------------------------------
// S3BlobStore – S3-compatible storage (RustFS / MinIO / AWS S3)
// ---------------------------------------------------------------------------

pub struct S3BlobStore {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl S3BlobStore {
    /// Create a new S3BlobStore.
    ///
    /// `endpoint_url` – the S3-compatible endpoint (e.g. `http://localhost:9000`).
    /// `bucket`       – the target bucket name.
    /// `region`       – AWS region (use any value for non-AWS backends).
    pub async fn new(endpoint_url: &str, bucket: &str, region: &str) -> anyhow::Result<Self> {
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .endpoint_url(endpoint_url)
            .region(aws_sdk_s3::config::Region::new(region.to_owned()))
            .load()
            .await;

        let s3_config = aws_sdk_s3::config::Builder::from(&config)
            .force_path_style(true)
            .build();

        let client = aws_sdk_s3::Client::from_conf(s3_config);

        // Ensure the bucket exists (ignore error if it already does)
        let _ = client.create_bucket().bucket(bucket).send().await;

        Ok(Self {
            client,
            bucket: bucket.to_owned(),
        })
    }

}

#[async_trait]
impl BlobStore for S3BlobStore {
    async fn put(&self, key: &str, bytes: Bytes, content_type: Option<&str>) -> anyhow::Result<()> {
        let mut req = self.client.put_object().bucket(&self.bucket).key(key);
        if let Some(ct) = content_type {
            req = req.content_type(ct);
        }
        req.body(bytes.into())
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("S3 put failed: {e}"))?;
        Ok(())
    }

    async fn get(&self, key: &str) -> anyhow::Result<Bytes> {
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("S3 get failed: {e}"))?;

        let data = resp
            .body
            .collect()
            .await
            .map_err(|e| anyhow::anyhow!("S3 body read failed: {e}"))?;

        Ok(data.into_bytes())
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("S3 delete failed: {e}"))?;
        Ok(())
    }

    async fn exists(&self, key: &str) -> anyhow::Result<bool> {
        match self.head(key).await {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn head(&self, key: &str) -> anyhow::Result<Option<i64>> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(resp) => Ok(resp.content_length()),
            Err(sdk_err) => {
                // NotFound → None, other errors → propagate
                let service_err = sdk_err.into_service_error();
                if service_err.is_not_found() {
                    Ok(None)
                } else {
                    Err(anyhow::anyhow!("S3 head failed: {service_err}"))
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// FileBlobStore – local file-system storage (legacy / development fallback)
// ---------------------------------------------------------------------------

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
    async fn put(
        &self,
        key: &str,
        bytes: Bytes,
        _content_type: Option<&str>,
    ) -> anyhow::Result<()> {
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

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        let path = self.root.join(key);
        if path.exists() {
            fs::remove_file(path).await?;
        }
        Ok(())
    }

    async fn exists(&self, key: &str) -> anyhow::Result<bool> {
        Ok(self.root.join(key).exists())
    }

    async fn head(&self, key: &str) -> anyhow::Result<Option<i64>> {
        let path = self.root.join(key);
        match fs::metadata(path).await {
            Ok(m) => Ok(Some(m.len() as i64)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

// ---------------------------------------------------------------------------
// MemoryBlobStore – in-memory storage for tests
// ---------------------------------------------------------------------------

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
    async fn put(
        &self,
        key: &str,
        bytes: Bytes,
        _content_type: Option<&str>,
    ) -> anyhow::Result<()> {
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

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        let mut storage = self.storage.write().await;
        storage.remove(key);
        Ok(())
    }

    async fn exists(&self, key: &str) -> anyhow::Result<bool> {
        let storage = self.storage.read().await;
        Ok(storage.contains_key(key))
    }

    async fn head(&self, key: &str) -> anyhow::Result<Option<i64>> {
        let storage = self.storage.read().await;
        Ok(storage.get(key).map(|b| b.len() as i64))
    }
}
