use std::collections::HashMap;
use std::hash::Hash;

#[derive(Debug, Clone)]
struct LruNode<K, V> {
    key: K,
    value: V,
    prev: Option<usize>,
    next: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct LruCache<K, V> {
    capacity: usize,
    map: HashMap<K, usize>,
    nodes: Vec<Option<LruNode<K, V>>>,
    free_indices: Vec<usize>,
    head: Option<usize>, // Most recently used
    tail: Option<usize>, // Least recently used
}

#[allow(dead_code)]
impl<K: Clone + Eq + Hash, V> LruCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            map: HashMap::new(),
            nodes: Vec::new(),
            free_indices: Vec::new(),
            head: None,
            tail: None,
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        if let Some(&idx) = self.map.get(key) {
            self.detach(idx);
            self.attach_head(idx);
            self.nodes[idx].as_ref().map(|n| &n.value)
        } else {
            None
        }
    }

    pub fn get_immutable(&self, key: &K) -> Option<&V> {
        self.map.get(key).and_then(|&idx| self.nodes[idx].as_ref().map(|n| &n.value))
    }

    pub fn insert(&mut self, key: K, value: V) {
        if let Some(&idx) = self.map.get(&key) {
            if let Some(node) = &mut self.nodes[idx] {
                node.value = value;
            }
            self.detach(idx);
            self.attach_head(idx);
            return;
        }

        if self.map.len() >= self.capacity {
            if let Some(lru_idx) = self.tail {
                if let Some(node) = self.nodes[lru_idx].take() {
                    self.map.remove(&node.key);
                }
                self.detach(lru_idx);
                self.free_indices.push(lru_idx);
            }
        }

        let idx = if let Some(free_idx) = self.free_indices.pop() {
            self.nodes[free_idx] = Some(LruNode {
                key: key.clone(),
                value,
                prev: None,
                next: None,
            });
            free_idx
        } else {
            let new_idx = self.nodes.len();
            self.nodes.push(Some(LruNode {
                key: key.clone(),
                value,
                prev: None,
                next: None,
            }));
            new_idx
        };

        self.map.insert(key, idx);
        self.attach_head(idx);
    }

    fn detach(&mut self, idx: usize) {
        let (prev, next) = match &self.nodes[idx] {
            Some(n) => (n.prev, n.next),
            None => return,
        };

        if let Some(p) = prev {
            if let Some(pn) = &mut self.nodes[p] {
                pn.next = next;
            }
        } else {
            self.head = next;
        }

        if let Some(n) = next {
            if let Some(nn) = &mut self.nodes[n] {
                nn.prev = prev;
            }
        } else {
            self.tail = prev;
        }

        if let Some(n) = &mut self.nodes[idx] {
            n.prev = None;
            n.next = None;
        }
    }

    fn attach_head(&mut self, idx: usize) {
        if let Some(old_head) = self.head {
            if let Some(n) = &mut self.nodes[idx] {
                n.next = Some(old_head);
                n.prev = None;
            }
            if let Some(ohn) = &mut self.nodes[old_head] {
                ohn.prev = Some(idx);
            }
            self.head = Some(idx);
        } else {
            if let Some(n) = &mut self.nodes[idx] {
                n.next = None;
                n.prev = None;
            }
            self.head = Some(idx);
            self.tail = Some(idx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_eviction_order() {
        let mut cache = LruCache::new(3);
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.insert("c", 3);
        assert_eq!(cache.len(), 3);

        // Access "a" so order becomes MRU -> "a", "c", "b" -> LRU
        assert_eq!(cache.get(&"a"), Some(&1));

        // Insert "d" -> should evict LRU "b"
        cache.insert("d", 4);
        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get(&"b"), None);
        assert_eq!(cache.get(&"a"), Some(&1));
        assert_eq!(cache.get(&"c"), Some(&3));
        assert_eq!(cache.get(&"d"), Some(&4));
    }
}
