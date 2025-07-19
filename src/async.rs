use parking_lot::Mutex as SyncMutex;
use std::{collections::HashMap, hash::Hash, sync::Arc};
use tokio::sync::{Mutex, OwnedMutexGuard};

/// An RAII implementation of a scoped lock. When this structure is dropped
/// (falls out of scope), the lock is released.
pub struct Guard<'k, K: Eq + Hash + Clone + Send> {
    key: K,
    _guard: OwnedMutexGuard<()>,
    keyed_lock: &'k KeyedLock<K>,
}

impl<'k, K: Eq + Hash + Clone + Send> Drop for Guard<'k, K> {
    fn drop(&mut self) {
        let mut registry = self.keyed_lock.0.lock();
        // If the strong count is 2, it means only the registry and current guard
        // hold a reference to the Arc. In this case, we can safely remove the
        // key from the registry.
        if let Some(arc_mutex) = registry.get(&self.key) {
            if Arc::strong_count(arc_mutex) == 2 {
                registry.remove(&self.key);
            }
        }
    }
}

/// An RAII implementation of a scoped lock for an `Arc<KeyedLock>`. When this
/// structure is dropped (falls out of scope), the lock is released.
pub struct OwnedGuard<K: Eq + Hash + Clone + Send> {
    key: K,
    _guard: OwnedMutexGuard<()>,
    keyed_lock: Arc<KeyedLock<K>>,
}

impl<K: Eq + Hash + Clone + Send> Drop for OwnedGuard<K> {
    fn drop(&mut self) {
        let mut registry = self.keyed_lock.0.lock();
        // If the strong count is 2, it means only the registry and current guard
        // hold a reference to the Arc. In this case, we can safely remove the
        // key from the registry.
        if let Some(arc_mutex) = registry.get(&self.key) {
            if Arc::strong_count(arc_mutex) == 2 {
                registry.remove(&self.key);
            }
        }
    }
}

/// A lock that provides mutually exclusive access to a resource, where the
/// resource is identified by a key.
pub struct KeyedLock<K: Eq + Hash + Clone + Send>(SyncMutex<HashMap<K, Arc<Mutex<()>>>>);

impl<K: Eq + Hash + Clone + Send> KeyedLock<K> {
    /// Creates a new `KeyedLock`.
    #[must_use]
    pub fn new() -> Self {
        Self(SyncMutex::new(HashMap::new()))
    }

    /// Acquires a lock for a given key.
    ///
    /// If the lock is already held by another task, this method will wait until
    /// the lock is released.
    ///
    /// When the returned `Guard` is dropped, the lock is released.
    pub async fn lock<'a>(&'a self, key: K) -> Guard<'a, K> {
        let _guard = self.lock_inner(&key).await;
        Guard {
            key,
            _guard,
            keyed_lock: self,
        }
    }

    /// Acquires a lock for a given key, returning an `OwnedGuard`.
    ///
    /// This method is for use with `Arc<KeyedLock>`. If the lock is already
    /// held by another task, this method will wait until the lock is released.
    ///
    /// When the returned `OwnedGuard` is dropped, the lock is released.
    pub async fn lock_owned(self: &Arc<Self>, key: K) -> OwnedGuard<K> {
        let _guard = self.lock_inner(&key).await;
        OwnedGuard {
            key,
            _guard,
            keyed_lock: self.clone(),
        }
    }

    /// Gets or creates a mutex for a key and locks it.
    async fn lock_inner(&self, key: &K) -> OwnedMutexGuard<()> {
        let key_lock = {
            let mut registry = self.0.lock();
            if let Some(notifies) = registry.get_mut(key) {
                Arc::clone(notifies)
            } else {
                let new = Arc::new(Mutex::new(()));
                registry.insert(key.clone(), new.clone());
                new
            }
        };
        key_lock.lock_owned().await
    }

    #[cfg(test)]
    fn registry_len(&self) -> usize {
        self.0.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_lock_unlock() {
        let keyed_lock = KeyedLock::new();
        let guard = keyed_lock.lock(1).await;
        drop(guard);
    }

    #[tokio::test]
    async fn test_lock_contention() {
        let keyed_lock = Arc::new(KeyedLock::new());
        let keyed_lock_clone = Arc::clone(&keyed_lock);

        let guard1 = keyed_lock.lock(1).await;

        let task = tokio::spawn(async move {
            keyed_lock_clone.lock(1).await;
        });

        sleep(Duration::from_millis(10)).await;
        assert!(!task.is_finished());

        drop(guard1);
        sleep(Duration::from_millis(10)).await;
        assert!(task.is_finished());
    }

    #[tokio::test]
    async fn test_owned_lock_unlock() {
        let keyed_lock = Arc::new(KeyedLock::new());
        let guard = keyed_lock.lock_owned(1).await;
        drop(guard);
    }

    #[tokio::test]
    async fn test_registry_cleanup() {
        let keyed_lock = KeyedLock::new();
        assert_eq!(keyed_lock.registry_len(), 0);

        let guard = keyed_lock.lock(1).await;
        assert_eq!(keyed_lock.registry_len(), 1);
        drop(guard);

        assert_eq!(keyed_lock.registry_len(), 0);
    }

    #[tokio::test]
    async fn test_multiple_keys() {
        let keyed_lock = KeyedLock::new();
        let guard1 = keyed_lock.lock(1).await;
        let guard2 = keyed_lock.lock(2).await;

        assert_eq!(keyed_lock.registry_len(), 2);

        drop(guard1);
        assert_eq!(keyed_lock.registry_len(), 1);

        drop(guard2);
        assert_eq!(keyed_lock.registry_len(), 0);
    }
}
