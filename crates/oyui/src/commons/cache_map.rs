use crate::commons::lazy::Lazy;
use scc::HashMap;
use std::path::PathBuf;

#[derive(Clone)]
pub struct CacheMap<T> {
    inner: HashMap<PathBuf, Lazy<T>>,
}

impl<T> Default for CacheMap<T> {
    fn default() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
}

impl<T: Clone> CacheMap<T> {
    pub fn clear(&self) {
        self.inner.clear_sync();
    }

    pub fn get(&self, path: &PathBuf) -> Lazy<T> {
        let res = self.inner.read_sync(path, |_, v| Lazy::<T>::clone(v));
        res.unwrap_or(Lazy::Uninitialized)
    }

    pub fn set(&self, path: PathBuf, value: T) {
        self.inner.upsert_sync(path, Lazy::Ready(value));
    }

    pub fn invalidate(&self, path: &PathBuf) {
        self.inner.update_sync(path, |_, v| {
            v.invalidate();
        });
    }

    pub fn mark_started(&self, path: PathBuf) {
        let _ = self.inner.insert_sync(path, Lazy::Started);
    }

    pub fn inner(&self) -> &HashMap<PathBuf, Lazy<T>> {
        &self.inner
    }
}
