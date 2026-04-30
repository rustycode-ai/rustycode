// ── LRU Cache for Context Items ───────────────────────────────────────────────

use std::collections::VecDeque;

/// A simple Least Recently Used (LRU) cache for context items.
///
/// This cache automatically evicts the least recently used items when
/// the capacity is exceeded.
///
/// # Type Parameters
///
/// * `K` - Key type (must implement Hash + Eq + Clone)
/// * `V` - Value type
///
/// # Example
///
/// ```
/// use rustycode_core::context::LruCache;
///
/// let mut cache = LruCache::new(3);
/// cache.insert("key1", "value1");
/// cache.insert("key2", "value2");
/// cache.insert("key3", "value3");
///
/// // This will evict key1 (least recently used)
/// cache.insert("key4", "value4");
///
/// assert!(!cache.contains_key(&"key1"));
/// assert!(cache.contains_key(&"key4"));
/// ```
#[derive(Debug, Clone)]
pub struct LruCache<K, V>
where
    K: Clone + std::hash::Hash + Eq,
{
    /// Maximum number of items to store
    capacity: usize,
    /// Key-value pairs (front = most recently used, back = least recently used)
    entries: VecDeque<(K, V)>,
}

impl<K, V> LruCache<K, V>
where
    K: Clone + std::hash::Hash + Eq,
{
    /// Create a new LRU cache with the specified capacity.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of items to store
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: VecDeque::with_capacity(capacity),
        }
    }

    /// Insert a key-value pair, evicting the least recently used item if necessary.
    ///
    /// # Arguments
    ///
    /// * `key` - Key to insert
    /// * `value` - Value to associate with the key
    pub fn insert(&mut self, key: K, value: V) {
        // Remove existing entry if present
        self.remove(&key);

        // Add to front (most recently used)
        self.entries.push_front((key, value));

        // Evict least recently used if over capacity
        if self.entries.len() > self.capacity {
            self.entries.pop_back();
        }
    }

    /// Get a value by key, marking it as most recently used.
    ///
    /// # Arguments
    ///
    /// * `key` - Key to look up
    ///
    /// # Returns
    ///
    /// * `Some(&value)` if found
    /// * `None` if not found
    pub fn get(&mut self, key: &K) -> Option<&V> {
        // Find the entry
        let pos = self.entries.iter().position(|(k, _)| k == key)?;

        // Remove and reinsert at front (mark as recently used)
        let (k, v) = self.entries.remove(pos).unwrap();
        self.entries.push_front((k, v));

        // Return reference to value
        self.entries.front().map(|(_, v)| v)
    }

    /// Remove an entry by key.
    ///
    /// # Arguments
    ///
    /// * `key` - Key to remove
    ///
    /// # Returns
    ///
    /// * `Some(value)` if found and removed
    /// * `None` if not found
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let pos = self.entries.iter().position(|(k, _)| k == key)?;
        self.entries.remove(pos).map(|(_, v)| v)
    }

    /// Check if a key exists in the cache.
    ///
    /// # Arguments
    ///
    /// * `key` - Key to check
    ///
    /// # Returns
    ///
    /// * `true` if key exists
    /// * `false` if not found
    pub fn contains_key(&self, key: &K) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }

    /// Get the number of items in the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries from the cache.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get all keys in the cache (from most to least recently used).
    pub fn keys(&self) -> Vec<&K> {
        self.entries.iter().map(|(k, _)| k).collect()
    }

    /// Get all values in the cache (from most to least recently used).
    pub fn values(&self) -> Vec<&V> {
        self.entries.iter().map(|(_, v)| v).collect()
    }

    /// Resize the cache capacity, evicting items if necessary.
    ///
    /// # Arguments
    ///
    /// * `new_capacity` - New maximum capacity
    pub fn resize(&mut self, new_capacity: usize) {
        self.capacity = new_capacity;

        // Evict excess items from the back (least recently used)
        while self.entries.len() > self.capacity {
            self.entries.pop_back();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_insert() {
        let mut cache = LruCache::new(3);
        cache.insert(1, "a");
        cache.insert(2, "b");
        cache.insert(3, "c");

        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn test_lru_eviction() {
        let mut cache = LruCache::new(2);
        cache.insert(1, "a");
        cache.insert(2, "b");
        cache.insert(3, "c"); // Evicts key 1

        assert!(!cache.contains_key(&1));
        assert!(cache.contains_key(&2));
        assert!(cache.contains_key(&3));
    }

    #[test]
    fn test_lru_get_updates_order() {
        let mut cache = LruCache::new(3);
        cache.insert(1, "a");
        cache.insert(2, "b");
        cache.insert(3, "c");

        cache.get(&1); // Move 1 to front

        cache.insert(4, "d"); // Evicts key 2 (now least recently used)

        assert!(cache.contains_key(&1)); // Still present (was accessed)
        assert!(!cache.contains_key(&2)); // Evicted
        assert!(cache.contains_key(&3));
        assert!(cache.contains_key(&4));
    }

    #[test]
    fn test_lru_remove() {
        let mut cache = LruCache::new(3);
        cache.insert(1, "a");
        cache.insert(2, "b");

        let removed = cache.remove(&1);
        assert_eq!(removed, Some("a"));
        assert!(!cache.contains_key(&1));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_lru_contains_key() {
        let mut cache = LruCache::new(3);
        cache.insert(1, "a");

        assert!(cache.contains_key(&1));
        assert!(!cache.contains_key(&2));
    }

    #[test]
    fn test_lru_clear() {
        let mut cache = LruCache::new(3);
        cache.insert(1, "a");
        cache.insert(2, "b");

        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_lru_keys() {
        let mut cache = LruCache::new(3);
        cache.insert(1, "a");
        cache.insert(2, "b");

        let keys = cache.keys();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&&2)); // Most recent
        assert!(keys.contains(&&1));
    }

    #[test]
    fn test_lru_values() {
        let mut cache = LruCache::new(3);
        cache.insert(1, "a");
        cache.insert(2, "b");

        let values = cache.values();
        assert_eq!(values.len(), 2);
        assert!(values.contains(&&"b")); // Most recent
        assert!(values.contains(&&"a"));
    }

    #[test]
    fn test_lru_resize() {
        let mut cache = LruCache::new(3);
        cache.insert(1, "a");
        cache.insert(2, "b");
        cache.insert(3, "c");

        cache.resize(2); // Evicts least recently used (key 1)

        assert_eq!(cache.len(), 2);
        assert!(!cache.contains_key(&1));
        assert!(cache.contains_key(&2));
        assert!(cache.contains_key(&3));
    }

    #[test]
    fn test_lru_resize_up() {
        let mut cache = LruCache::new(2);
        cache.insert(1, "a");
        cache.insert(2, "b");

        cache.resize(5); // Should not evict anything

        assert_eq!(cache.len(), 2);
        assert!(cache.contains_key(&1));
        assert!(cache.contains_key(&2));
    }
}
