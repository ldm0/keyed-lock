use parking_lot::{ArcMutexGuard, Mutex, RawMutex};
use std::{collections::HashMap, hash::Hash, sync::Arc};

/// An RAII implementation of a scoped lock. When this structure is dropped
/// (falls out of scope), the lock is released.
pub struct Guard<'k, K: Eq + Hash + Clone> {
    key: K,
    _guard: ArcMutexGuard<RawMutex, ()>,
    keyed_lock: &'k KeyedLock<K>,
}

impl<'k, K: Eq + Hash + Clone> Drop for Guard<'k, K> {
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
pub struct OwnedGuard<K: Eq + Hash + Clone> {
    key: K,
    _guard: ArcMutexGuard<RawMutex, ()>,
    keyed_lock: Arc<KeyedLock<K>>,
}

impl<K: Eq + Hash + Clone> Drop for OwnedGuard<K> {
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
pub struct KeyedLock<K: Eq + Hash + Clone>(Mutex<HashMap<K, Arc<Mutex<()>>>>);

impl<K: Eq + Hash + Clone> KeyedLock<K> {
    /// Creates a new `KeyedLock`.
    #[must_use]
    pub fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    /// Acquires a lock for a given key.
    ///
    /// If the lock is already held by another task, this method will wait until
    /// the lock is released.
    ///
    /// When the returned `Guard` is dropped, the lock is released.
    pub fn lock(&self, key: K) -> Guard<'_, K> {
        let _guard = self.lock_inner(&key);
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
    pub fn lock_owned(self: &Arc<Self>, key: K) -> OwnedGuard<K> {
        let _guard = self.lock_inner(&key);
        OwnedGuard {
            key,
            _guard,
            keyed_lock: self.clone(),
        }
    }

    /// Gets or creates a mutex for a key and locks it.
    fn lock_inner(&self, key: &K) -> ArcMutexGuard<RawMutex, ()> {
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
        key_lock.lock_arc()
    }

    #[cfg(test)]
    fn registry_len(&self) -> usize {
        self.0.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn test_basic_lock() {
        let keyed_lock = KeyedLock::new();
        let _guard = keyed_lock.lock(1);
        // The lock is held here.
        // When _guard goes out of scope, the lock is released.
    }

    #[test]
    fn test_concurrent_access() {
        let keyed_lock = Arc::new(KeyedLock::new());
        let mut handles = vec![];

        for _ in 0..10 {
            let keyed_lock_clone = Arc::clone(&keyed_lock);
            let handle = thread::spawn(move || {
                let _guard = keyed_lock_clone.lock(1);
                // Do some work while holding the lock.
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_lock_is_released() {
        let keyed_lock = KeyedLock::new();
        let guard = keyed_lock.lock(1);
        drop(guard);
        // The lock should be released now.
        let _guard2 = keyed_lock.lock(1);
    }

    #[test]
    fn test_lock_reuse() {
        let keyed_lock = KeyedLock::new();
        let guard1 = keyed_lock.lock(1);
        drop(guard1);
        let guard2 = keyed_lock.lock(1);
        drop(guard2);
    }

    #[test]
    fn test_locks_different_keys() {
        let keyed_lock = KeyedLock::new();
        let _guard1 = keyed_lock.lock(1);
        let _guard2 = keyed_lock.lock(2);
        // Locks for different keys should not block each other.
    }

    #[test]
    fn test_multiple_keys_concurrently() {
        let keyed_lock = Arc::new(KeyedLock::new());
        let mut handles = vec![];

        for i in 0..10 {
            let keyed_lock_clone = Arc::clone(&keyed_lock);
            let handle = thread::spawn(move || {
                let _guard = keyed_lock_clone.lock(i);
                // Do some work while holding the lock.
                thread::sleep(Duration::from_millis(10));
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_non_reentrant_lock() {
        let keyed_lock = Arc::new(KeyedLock::new());
        let keyed_lock_clone = Arc::clone(&keyed_lock);

        // Acquire the lock in the main thread
        let _guard = keyed_lock.lock(1);

        // Try to acquire the same lock in another thread
        let handle = thread::spawn(move || {
            let now = Instant::now();
            let _guard = keyed_lock_clone.lock(1);
            assert!(now.elapsed() >= Duration::from_secs(3));
            // This part should not be reached if the lock is non-reentrant
        });

        std::thread::sleep(Duration::from_secs(4));
        drop(_guard);

        handle.join().unwrap();
    }

    #[test]
    fn test_registry_cleanup() {
        let keyed_lock = KeyedLock::new();
        assert_eq!(keyed_lock.registry_len(), 0);

        // Lock a key, registry should have one entry.
        let guard = keyed_lock.lock(1);
        assert_eq!(keyed_lock.registry_len(), 1);

        // Drop the guard, registry should be empty.
        drop(guard);
        assert_eq!(keyed_lock.registry_len(), 0);
    }

    #[test]
    fn test_registry_cleanup_concurrent() {
        let keyed_lock = Arc::new(KeyedLock::new());
        assert_eq!(keyed_lock.registry_len(), 0);

        let guard1 = keyed_lock.lock(1);
        assert_eq!(keyed_lock.registry_len(), 1);

        let keyed_lock_clone = Arc::clone(&keyed_lock);
        let handle = thread::spawn(move || {
            // This will block until guard1 is dropped.
            let guard2 = keyed_lock_clone.lock(1);
            // The registry should still contain the key.
            assert_eq!(keyed_lock_clone.registry_len(), 1);
            drop(guard2);
        });

        // Before dropping guard1, another thread is waiting. Registry should have 1 entry.
        // The strong count of the Arc is 3 (registry, guard1, handle's closure).
        assert_eq!(keyed_lock.registry_len(), 1);
        drop(guard1);

        handle.join().unwrap();

        // After all guards are dropped, the registry should be empty.
        assert_eq!(keyed_lock.registry_len(), 0);
    }

    #[test]
    fn test_registry_cleanup_arc() {
        let keyed_lock = Arc::new(KeyedLock::new());
        assert_eq!(keyed_lock.registry_len(), 0);

        // Lock a key, registry should have one entry.
        let guard = keyed_lock.lock_owned(1);
        assert_eq!(keyed_lock.registry_len(), 1);

        // Drop the guard, registry should be empty.
        drop(guard);
        assert_eq!(keyed_lock.registry_len(), 0);
    }

    #[test]
    fn test_lock_arc_concurrently() {
        let keyed_lock = Arc::new(KeyedLock::new());
        let mut handles = vec![];

        for i in 0..10 {
            let keyed_lock_clone = Arc::clone(&keyed_lock);
            let handle = thread::spawn(move || {
                let _guard = keyed_lock_clone.lock_owned(i);
                // Do some work while holding the lock.
                thread::sleep(Duration::from_millis(10));
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[cfg(feature = "send_guard")]
    #[test]
    fn test_non_reentrant_lock_arc() {
        let keyed_lock = Arc::new(KeyedLock::new());

        // Acquire the lock in the main thread
        let _guard = keyed_lock.lock_owned(1);

        // Try to acquire the same lock in another thread
        let handle = thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(4));
            drop(_guard);
        });

        let now = Instant::now();
        let _guard = keyed_lock.lock(1);
        assert!(now.elapsed() >= Duration::from_secs(4));

        handle.join().unwrap();
    }

    #[test]
    fn test_basic_lock_arc() {
        let keyed_lock = Arc::new(KeyedLock::new());
        let _guard = keyed_lock.lock_owned(1);
        // The lock is held here.
        // When _guard goes out of scope, the lock is released.
    }

    #[test]
    fn test_lock_is_released_arc() {
        let keyed_lock = Arc::new(KeyedLock::new());
        let guard = keyed_lock.lock_owned(1);
        drop(guard);
        // The lock should be released now.
        let _guard2 = keyed_lock.lock_owned(1);
    }

    #[test]
    fn test_lock_reuse_arc() {
        let keyed_lock = Arc::new(KeyedLock::new());
        let guard1 = keyed_lock.lock_owned(1);
        drop(guard1);
        let guard2 = keyed_lock.lock_owned(1);
        drop(guard2);
    }

    #[test]
    fn test_locks_different_keys_arc() {
        let keyed_lock = Arc::new(KeyedLock::new());
        let _guard1 = keyed_lock.lock_owned(1);
        let _guard2 = keyed_lock.lock_owned(2);
        // Locks for different keys should not block each other.
    }
}
