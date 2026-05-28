use crate::commons::lazy::Lazy;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug)]
pub struct CacheMap<T> {
    inner: HashMap<PathBuf, Lazy<T>>,
}
impl<T> Default for CacheMap<T> {
    fn default() -> Self {
        Self {
            inner: Default::default(),
        }
    }
}

impl<T> CacheMap<T> {
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    pub fn get(&self, path: &PathBuf) -> &Lazy<T> {
        self.inner.get(path).unwrap_or(&Lazy::Uninitialized)
    }

    pub fn set(&mut self, path: PathBuf, value: T) {
        self.inner.insert(path, Lazy::Ready(value));
    }

    pub fn invalidate(&mut self, path: &PathBuf) {
        if let Some(entry) = self.inner.get_mut(path) {
            entry.invalidate();
        }
    }

    pub fn mark_started(&mut self, path: PathBuf) {
        self.inner.entry(path).or_insert(Lazy::Started);
    }

    pub fn iter(&self) -> std::collections::hash_map::Values<'_, PathBuf, Lazy<T>> {
        self.inner.values()
    }
}
