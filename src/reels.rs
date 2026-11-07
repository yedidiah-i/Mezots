//! Published clips.
//!
//! A reel is a short vertical video that a student recorded in the browser
//! and handed to the node. The node keeps the file on disk and serves it
//! back over HTTP, so the whole feed lives inside the school network and
//! costs the school nothing to share.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use rand::RngCore;
use serde::{Deserialize, Serialize};

/// The description of a published clip.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reel {
    /// Stable id of the clip.
    pub id: String,
    /// The caption.
    pub title: String,
    /// Who published it.
    pub author: String,
    /// The node id of the publisher.
    pub author_peer_id: String,
    /// MIME type of the file.
    pub mime: String,
    /// The path the node serves it from.
    pub url: String,
    /// When it was published.
    pub ts: u64,
    /// How long the clip runs, in milliseconds.
    pub duration_ms: u64,
    /// How many students liked it.
    pub likes: u64,
    /// How many comments it has.
    pub comments: u64,
    /// The block that committed to this clip, when it is known.
    pub block_hash: Option<String>,
}

/// The store of published clips.
#[derive(Debug, Clone)]
pub struct ReelStore {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    dir: PathBuf,
    reels: Mutex<Vec<Reel>>,
}

impl ReelStore {
    /// Open a store in the given directory, creating it when needed.
    pub fn open(dir: impl Into<PathBuf>) -> std::io::Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            inner: Arc::new(Inner {
                dir,
                reels: Mutex::new(Vec::new()),
            }),
        })
    }

    /// The directory the clips live in.
    pub fn dir(&self) -> &Path {
        &self.inner.dir
    }

    /// Every published clip, newest first.
    pub fn list(&self) -> Vec<Reel> {
        let mut reels = self.inner.reels.lock();
        reels.sort_by(|a, b| b.ts.cmp(&a.ts));
        reels.clone()
    }

    /// Store a clip and return its description.
    pub fn store(
        &self,
        title: &str,
        mime: &str,
        bytes: &[u8],
        author: &str,
        author_peer_id: &str,
    ) -> std::io::Result<Reel> {
        let id = reel_id();
        let extension = extension_for(mime);
        let file = self.inner.dir.join(format!("{id}.{extension}"));
        std::fs::write(&file, bytes)?;

        let reel = Reel {
            id: id.clone(),
            title: title.to_string(),
            author: author.to_string(),
            author_peer_id: author_peer_id.to_string(),
            mime: mime.to_string(),
            url: format!("/reels/{id}.{extension}"),
            ts: now_ms(),
            duration_ms: 0,
            likes: 0,
            comments: 0,
            block_hash: None,
        };

        self.inner.reels.lock().push(reel.clone());
        Ok(reel)
    }

    /// Find the file that belongs to a served path.
    ///
    /// The path is checked against the directory so that a request can never
    /// walk out of it.
    pub fn file_for(&self, name: &str) -> Option<PathBuf> {
        if name.contains('/') || name.contains('\\') || name.contains("..") {
            return None;
        }
        let candidate = self.inner.dir.join(name);
        if candidate.starts_with(&self.inner.dir) && candidate.is_file() {
            Some(candidate)
        } else {
            None
        }
    }

    /// Record the block that committed to a clip.
    pub fn attach_block(&self, id: &str, hash: &str) {
        let mut reels = self.inner.reels.lock();
        if let Some(reel) = reels.iter_mut().find(|reel| reel.id == id) {
            reel.block_hash = Some(hash.to_string());
        }
    }
}

/// The current time in milliseconds since the epoch.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A random id for a clip.
fn reel_id() -> String {
    let mut bytes = [0u8; 6];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The file extension that belongs to a MIME type.
fn extension_for(mime: &str) -> String {
    match mime.split(';').next().unwrap_or("").trim() {
        "video/webm" => "webm".to_string(),
        "video/mp4" => "mp4".to_string(),
        "video/quicktime" => "mov".to_string(),
        "image/png" => "png".to_string(),
        "image/jpeg" => "jpg".to_string(),
        "image/gif" => "gif".to_string(),
        _ => "bin".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clip_is_stored_and_listed() {
        let dir = std::env::temp_dir().join(format!("mezots-reels-{}", reel_id()));
        let store = ReelStore::open(&dir).expect("the store should open");

        let reel = store
            .store("Hello", "video/webm", b"fake bytes", "Ada", "node-a")
            .expect("the clip should be stored");

        assert_eq!(reel.title, "Hello");
        assert_eq!(reel.url, format!("/reels/{}.webm", reel.id));
        assert_eq!(store.list().len(), 1);
        assert!(store.file_for(&format!("{}.webm", reel.id)).is_some());
    }

    #[test]
    fn a_path_cannot_escape_the_directory() {
        let dir = std::env::temp_dir().join(format!("mezots-reels-{}", reel_id()));
        let store = ReelStore::open(&dir).expect("the store should open");
        assert!(store.file_for("../secret.txt").is_none());
        assert!(store.file_for("sub/dir.txt").is_none());
    }
}
