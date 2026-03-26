use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::fs;
use tracing::instrument;

use umms_core::error::{StorageError, UmmsError};
use umms_core::traits::RawFileStore;
use umms_core::types::AgentId;

/// Local filesystem implementation of [`RawFileStore`].
///
/// Files are stored under `{base_dir}/{agent_id}/{filename}`, providing
/// natural per-agent isolation. The relative path `{agent_id}/{filename}`
/// is used as the canonical storage path for read/delete/exists operations.
#[derive(Debug, Clone)]
pub struct LocalFileStore {
    base_dir: PathBuf,
}

impl LocalFileStore {
    /// Create a new `LocalFileStore` rooted at `base_dir`.
    ///
    /// The directory is created (including parents) if it does not exist.
    pub async fn new(base_dir: impl Into<PathBuf>) -> Result<Self, UmmsError> {
        let base_dir = base_dir.into();
        fs::create_dir_all(&base_dir)
            .await
            .map_err(StorageError::Io)?;
        Ok(Self { base_dir })
    }

    /// Validate that a path component does not contain traversal sequences.
    fn validate_path(path: &str) -> Result<(), UmmsError> {
        if path.contains("..") {
            return Err(StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "path traversal detected: path must not contain '..'",
            ))
            .into());
        }
        Ok(())
    }

    /// Resolve a relative storage path to an absolute filesystem path.
    fn resolve(&self, path: &str) -> PathBuf {
        self.base_dir.join(Path::new(path))
    }
}

#[async_trait]
impl RawFileStore for LocalFileStore {
    #[instrument(skip(self, data), fields(agent_id = %agent_id, filename = %filename, size = data.len()))]
    async fn store(
        &self,
        agent_id: &AgentId,
        filename: &str,
        data: &[u8],
    ) -> Result<String, UmmsError> {
        Self::validate_path(filename)?;
        Self::validate_path(agent_id.as_str())?;

        let agent_dir = self.base_dir.join(agent_id.as_str());
        fs::create_dir_all(&agent_dir)
            .await
            .map_err(StorageError::Io)?;

        let file_path = agent_dir.join(filename);
        fs::write(&file_path, data)
            .await
            .map_err(StorageError::Io)?;

        // Return the relative storage path using forward slashes for consistency.
        let relative = format!("{}/{}", agent_id.as_str(), filename);
        Ok(relative)
    }

    #[instrument(skip(self))]
    async fn read(&self, path: &str) -> Result<Vec<u8>, UmmsError> {
        Self::validate_path(path)?;
        let abs = self.resolve(path);
        let data = fs::read(&abs).await.map_err(StorageError::Io)?;
        Ok(data)
    }

    #[instrument(skip(self))]
    async fn delete(&self, path: &str) -> Result<(), UmmsError> {
        Self::validate_path(path)?;
        let abs = self.resolve(path);
        fs::remove_file(&abs).await.map_err(StorageError::Io)?;
        Ok(())
    }

    #[instrument(skip(self))]
    async fn exists(&self, path: &str) -> Result<bool, UmmsError> {
        Self::validate_path(path)?;
        let abs = self.resolve(path);
        Ok(abs.exists())
    }

    #[instrument(skip(self), fields(agent_id = %agent_id))]
    async fn list(&self, agent_id: &AgentId) -> Result<Vec<String>, UmmsError> {
        let agent_dir = self.base_dir.join(agent_id.as_str());
        if !agent_dir.exists() {
            return Ok(Vec::new());
        }

        let mut entries = Vec::new();
        let mut reader = fs::read_dir(&agent_dir)
            .await
            .map_err(StorageError::Io)?;

        while let Some(entry) = reader.next_entry().await.map_err(StorageError::Io)? {
            if let Some(name) = entry.file_name().to_str() {
                entries.push(format!("{}/{}", agent_id.as_str(), name));
            }
        }

        entries.sort();
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use uuid::Uuid;

    /// Create a unique temporary directory for a test.
    fn test_dir() -> PathBuf {
        let dir = env::temp_dir().join(format!("umms-test-{}", Uuid::new_v4()));
        dir
    }

    /// Clean up a test directory.
    async fn cleanup(dir: &Path) {
        let _ = fs::remove_dir_all(dir).await;
    }

    #[tokio::test]
    async fn store_and_read_back() {
        let dir = test_dir();
        let store = LocalFileStore::new(&dir).await.unwrap();
        let agent = AgentId::from_str("agent-1").unwrap();
        let data = b"hello world";

        let path = store.store(&agent, "test.txt", data).await.unwrap();
        let read_back = store.read(&path).await.unwrap();

        assert_eq!(read_back, data);

        cleanup(&dir).await;
    }

    #[tokio::test]
    async fn store_creates_agent_subdirectory() {
        let dir = test_dir();
        let store = LocalFileStore::new(&dir).await.unwrap();
        let agent = AgentId::from_str("new-agent").unwrap();

        store.store(&agent, "file.bin", b"data").await.unwrap();

        let agent_dir = dir.join("new-agent");
        assert!(agent_dir.is_dir());

        cleanup(&dir).await;
    }

    #[tokio::test]
    async fn files_are_isolated_by_agent() {
        let dir = test_dir();
        let store = LocalFileStore::new(&dir).await.unwrap();

        let agent_a = AgentId::from_str("agent-a").unwrap();
        let agent_b = AgentId::from_str("agent-b").unwrap();

        let path_a = store.store(&agent_a, "data.bin", b"aaa").await.unwrap();
        let path_b = store.store(&agent_b, "data.bin", b"bbb").await.unwrap();

        assert_ne!(path_a, path_b);
        assert_eq!(store.read(&path_a).await.unwrap(), b"aaa");
        assert_eq!(store.read(&path_b).await.unwrap(), b"bbb");

        cleanup(&dir).await;
    }

    #[tokio::test]
    async fn delete_removes_file() {
        let dir = test_dir();
        let store = LocalFileStore::new(&dir).await.unwrap();
        let agent = AgentId::from_str("agent-del").unwrap();

        let path = store.store(&agent, "gone.txt", b"bye").await.unwrap();
        assert!(store.exists(&path).await.unwrap());

        store.delete(&path).await.unwrap();
        assert!(!store.exists(&path).await.unwrap());

        cleanup(&dir).await;
    }

    #[tokio::test]
    async fn exists_returns_correct_bool() {
        let dir = test_dir();
        let store = LocalFileStore::new(&dir).await.unwrap();
        let agent = AgentId::from_str("agent-ex").unwrap();

        assert!(!store.exists("agent-ex/nope.txt").await.unwrap());

        store.store(&agent, "yes.txt", b"here").await.unwrap();
        assert!(store.exists("agent-ex/yes.txt").await.unwrap());

        cleanup(&dir).await;
    }

    #[tokio::test]
    async fn path_traversal_is_rejected() {
        let dir = test_dir();
        let store = LocalFileStore::new(&dir).await.unwrap();
        let agent = AgentId::from_str("agent-pt").unwrap();

        // Traversal in filename
        let result = store.store(&agent, "../escape.txt", b"bad").await;
        assert!(result.is_err());

        // Traversal in read path
        let result = store.read("../etc/passwd").await;
        assert!(result.is_err());

        // Traversal in delete path
        let result = store.delete("agent-pt/../../etc/passwd").await;
        assert!(result.is_err());

        // Traversal in exists path
        let result = store.exists("../outside").await;
        assert!(result.is_err());

        cleanup(&dir).await;
    }

    #[tokio::test]
    async fn reading_nonexistent_file_returns_io_error() {
        let dir = test_dir();
        let store = LocalFileStore::new(&dir).await.unwrap();

        let result = store.read("no-agent/missing.txt").await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        match err {
            UmmsError::Storage(StorageError::Io(_)) => {} // expected
            other => panic!("expected StorageError::Io, got: {other:?}"),
        }

        cleanup(&dir).await;
    }
}
