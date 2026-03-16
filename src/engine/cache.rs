use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

const CACHE_VERSION: u32 = 3;

#[derive(Clone)]
pub struct DiskCache {
    dir: PathBuf,
}

impl DiskCache {
    pub fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let dir = PathBuf::from(home).join(".lazyspec").join("cache");
        let _ = fs::create_dir_all(&dir);
        DiskCache { dir }
    }

    #[cfg(test)]
    pub fn with_dir(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        DiskCache { dir }
    }

    fn path_hash(path: &Path) -> u64 {
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        hasher.finish()
    }

    fn cache_key(path: &Path, body_hash: u64) -> String {
        let path_h = Self::path_hash(path);
        format!("v{}_{:016x}_{:016x}", CACHE_VERSION, path_h, body_hash)
    }

    pub fn body_hash(body: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        body.hash(&mut hasher);
        hasher.finish()
    }

    pub fn read(&self, path: &Path, body_hash: u64) -> Option<String> {
        let key = Self::cache_key(path, body_hash);
        let file = self.dir.join(key);
        fs::read_to_string(file).ok()
    }

    pub fn write(&self, path: &Path, body_hash: u64, expanded: &str) {
        let key = Self::cache_key(path, body_hash);
        let file = self.dir.join(key);
        let _ = fs::write(file, expanded);
    }

    pub fn invalidate(&self, path: &Path) {
        let path_hash_str = format!("{:016x}", Self::path_hash(path));
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.contains(&path_hash_str) {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
        }
    }

    pub fn clear(&self) {
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_cache() -> (DiskCache, TempDir) {
        let tmp = TempDir::new().unwrap();
        let cache = DiskCache::with_dir(tmp.path().to_path_buf());
        (cache, tmp)
    }

    #[test]
    fn test_disk_cache_write_and_read() {
        let (cache, _tmp) = make_cache();
        let path = Path::new("rfcs/RFC-001.md");
        let body_hash = DiskCache::body_hash("raw body");
        let expanded = "expanded content with types";

        cache.write(path, body_hash, expanded);
        let result = cache.read(path, body_hash);

        assert_eq!(result, Some(expanded.to_string()));
    }

    #[test]
    fn test_disk_cache_miss() {
        let (cache, _tmp) = make_cache();
        let path = Path::new("rfcs/RFC-002.md");
        let body_hash = DiskCache::body_hash("some body");

        let result = cache.read(path, body_hash);

        assert_eq!(result, None);
    }

    #[test]
    fn test_disk_cache_invalidate() {
        let (cache, _tmp) = make_cache();
        let path = Path::new("rfcs/RFC-003.md");
        let body_hash = DiskCache::body_hash("body text");

        cache.write(path, body_hash, "expanded");
        cache.invalidate(path);
        let result = cache.read(path, body_hash);

        assert_eq!(result, None);
    }

    #[test]
    fn test_disk_cache_clear() {
        let (cache, _tmp) = make_cache();
        let path_a = Path::new("rfcs/RFC-004.md");
        let path_b = Path::new("stories/STORY-001.md");
        let hash_a = DiskCache::body_hash("body a");
        let hash_b = DiskCache::body_hash("body b");

        cache.write(path_a, hash_a, "expanded a");
        cache.write(path_b, hash_b, "expanded b");
        cache.clear();

        assert_eq!(cache.read(path_a, hash_a), None);
        assert_eq!(cache.read(path_b, hash_b), None);
    }

    #[test]
    fn test_disk_cache_different_body_hash() {
        let (cache, _tmp) = make_cache();
        let path = Path::new("rfcs/RFC-005.md");
        let hash_v1 = DiskCache::body_hash("version 1");
        let hash_v2 = DiskCache::body_hash("version 2");

        cache.write(path, hash_v1, "expanded v1");
        let result = cache.read(path, hash_v2);

        assert_eq!(result, None);
    }
}
