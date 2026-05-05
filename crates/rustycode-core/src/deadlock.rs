use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;

pub type LockId = u64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockType {
    Mutex,
    RwLockWrite,
    Semaphore,
}

#[derive(Clone, Debug)]
pub struct LockInfo {
    pub name: String,
    pub lock_type: LockType,
    pub state: LockState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockState {
    Free,
    Held,
}

#[derive(Clone, Debug)]
pub struct DetectorConfig {
    pub enable_cycle_detection: bool,
    pub enable_timeout_detection: bool,
    pub timeout_threshold: chrono::Duration,
    pub max_tracked_locks: usize,
    pub sampling_rate: f64,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self::builder().build()
    }
}

pub struct DetectorConfigBuilder {
    inner: DetectorConfig,
}

impl DetectorConfig {
    pub fn builder() -> DetectorConfigBuilder {
        DetectorConfigBuilder {
            inner: DetectorConfig {
                enable_cycle_detection: true,
                enable_timeout_detection: true,
                timeout_threshold: chrono::Duration::seconds(30),
                max_tracked_locks: 1000,
                sampling_rate: 1.0,
            },
        }
    }
}

impl DetectorConfigBuilder {
    pub fn enable_cycle_detection(mut self, v: bool) -> Self {
        self.inner.enable_cycle_detection = v;
        self
    }
    pub fn enable_timeout_detection(mut self, v: bool) -> Self {
        self.inner.enable_timeout_detection = v;
        self
    }
    pub fn timeout_threshold(mut self, d: chrono::Duration) -> Self {
        self.inner.timeout_threshold = d;
        self
    }
    pub fn max_tracked_locks(mut self, n: usize) -> Self {
        self.inner.max_tracked_locks = n;
        self
    }
    pub fn sampling_rate(mut self, r: f64) -> Self {
        self.inner.sampling_rate = r;
        self
    }
    pub fn build(self) -> DetectorConfig {
        self.inner
    }
}

#[derive(Debug, Default)]
pub struct DependencyGraph {
    adj: HashMap<LockId, HashSet<LockId>>,
}

impl DependencyGraph {
    pub fn add_dependency(&mut self, from: LockId, to: LockId) {
        self.adj.entry(from).or_default().insert(to);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeadlockType {
    None,
    CycleDetected,
    TimeoutDetected,
}

#[derive(Clone, Debug)]
pub struct DeadlockReport {
    pub deadlock_type: DeadlockType,
    pub involved_locks: Vec<LockId>,
    pub lock_names: Vec<String>,
    pub prevention_strategy: String,
    pub cycle_description: Option<String>,
}

impl DeadlockReport {
    pub fn has_deadlock(&self) -> bool {
        !matches!(self.deadlock_type, DeadlockType::None)
    }
}

#[derive(Clone, Debug)]
pub struct LockStatistics {
    pub total_locks: usize,
    pub total_acquisitions: usize,
}

pub struct DeadlockDetector {
    pub config: DetectorConfig,
    pub locks: RwLock<HashMap<LockId, LockInfo>>,
    pub graph: RwLock<DependencyGraph>,
    pub pending_acquisitions: RwLock<HashMap<LockId, DateTime<Utc>>>,
    next_id: AtomicU64,
    acquisition_count: RwLock<usize>,
}

impl Default for DeadlockDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl DeadlockDetector {
    pub fn new() -> Self {
        Self::with_config(DetectorConfig::default())
    }

    pub fn with_config(config: DetectorConfig) -> Self {
        DeadlockDetector {
            config,
            locks: RwLock::new(HashMap::new()),
            graph: RwLock::new(DependencyGraph::default()),
            pending_acquisitions: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            acquisition_count: RwLock::new(0),
        }
    }

    pub async fn register_lock(&self, name: String, lock_type: LockType) -> Result<LockId> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let info = LockInfo {
            name: name.clone(),
            lock_type,
            state: LockState::Free,
        };
        let mut locks = self.locks.write().await;
        locks.insert(id, info);
        Ok(id)
    }

    pub async fn record_acquisition(
        &self,
        lock_id: LockId,
        _task_id: u64,
        held: bool,
        _prio: Option<u32>,
    ) -> Result<()> {
        let mut locks = self.locks.write().await;
        if let Some(info) = locks.get_mut(&lock_id) {
            info.state = if held {
                LockState::Held
            } else {
                LockState::Free
            };
        }
        let mut cnt = self.acquisition_count.write().await;
        *cnt += 1;
        Ok(())
    }

    pub async fn lock_statistics(&self) -> LockStatistics {
        let locks = self.locks.read().await;
        let cnt = *self.acquisition_count.read().await;
        LockStatistics {
            total_locks: locks.len(),
            total_acquisitions: cnt,
        }
    }

    pub async fn detect_deadlocks(&self) -> DeadlockReport {
        // Simple cycle detection via DFS
        let graph = self.graph.read().await;
        let mut visited = HashSet::new();
        let mut stack = HashSet::new();
        let mut found_cycle = Vec::new();

        for &node in graph.adj.keys() {
            if Self::dfs_cycle(&graph.adj, node, &mut visited, &mut stack, &mut found_cycle) {
                break;
            }
        }

        if !found_cycle.is_empty() && self.config.enable_cycle_detection {
            // Map names
            let locks = self.locks.read().await;
            let names = found_cycle
                .iter()
                .filter_map(|id| locks.get(id).map(|li| li.name.clone()))
                .collect();
            return DeadlockReport {
                deadlock_type: DeadlockType::CycleDetected,
                involved_locks: found_cycle,
                lock_names: names,
                prevention_strategy: "Global Lock Ordering".to_string(),
                cycle_description: Some("cycle detected".to_string()),
            };
        }

        if self.config.enable_timeout_detection {
            let pending = self.pending_acquisitions.read().await;
            let now = Utc::now();
            for (&id, &ts) in pending.iter() {
                let elapsed = now - ts;
                if elapsed > self.config.timeout_threshold {
                    let locks = self.locks.read().await;
                    let name = locks
                        .get(&id)
                        .map(|li| li.name.clone())
                        .unwrap_or_else(|| "unknown".to_string());
                    return DeadlockReport {
                        deadlock_type: DeadlockType::TimeoutDetected,
                        involved_locks: vec![id],
                        lock_names: vec![name],
                        prevention_strategy: "Timeout monitoring".to_string(),
                        cycle_description: None,
                    };
                }
            }
        }

        DeadlockReport {
            deadlock_type: DeadlockType::None,
            involved_locks: Vec::new(),
            lock_names: Vec::new(),
            prevention_strategy: String::new(),
            cycle_description: None,
        }
    }

    fn dfs_cycle(
        adj: &HashMap<LockId, HashSet<LockId>>,
        node: LockId,
        visited: &mut HashSet<LockId>,
        stack: &mut HashSet<LockId>,
        found: &mut Vec<LockId>,
    ) -> bool {
        if !visited.insert(node) {
            return false;
        }
        stack.insert(node);
        if let Some(neis) = adj.get(&node) {
            for &n in neis {
                if !visited.contains(&n) {
                    if Self::dfs_cycle(adj, n, visited, stack, found) {
                        if !found.contains(&node) {
                            found.push(node);
                        }
                        return true;
                    }
                } else if stack.contains(&n) {
                    // found cycle
                    found.push(n);
                    found.push(node);
                    return true;
                }
            }
        }
        stack.remove(&node);
        false
    }

    pub async fn deadlock_reports(&self) -> Vec<DeadlockReport> {
        // For now, return single report if any
        let r = self.detect_deadlocks().await;
        if r.has_deadlock() {
            vec![r]
        } else {
            vec![]
        }
    }
}

// Minimal wrapper types expected by tests

pub struct MutexWrapper;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_builder_defaults() {
        let config = DetectorConfig::default();
        assert!(config.enable_cycle_detection);
        assert!(config.enable_timeout_detection);
        assert_eq!(config.max_tracked_locks, 1000);
    }

    #[test]
    fn config_builder_custom() {
        let config = DetectorConfig::builder()
            .enable_cycle_detection(false)
            .max_tracked_locks(500)
            .sampling_rate(0.5)
            .build();
        assert!(!config.enable_cycle_detection);
        assert_eq!(config.max_tracked_locks, 500);
    }

    #[tokio::test]
    async fn register_and_acquire_lock() {
        let detector = DeadlockDetector::new();
        let id = detector
            .register_lock("test_lock".into(), LockType::Mutex)
            .await
            .unwrap();
        assert!(id > 0);

        detector
            .record_acquisition(id, 1, true, None)
            .await
            .unwrap();

        let stats = detector.lock_statistics().await;
        assert_eq!(stats.total_locks, 1);
        assert_eq!(stats.total_acquisitions, 1);
    }

    #[tokio::test]
    async fn no_deadlock_on_empty_graph() {
        let detector = DeadlockDetector::new();
        let report = detector.detect_deadlocks().await;
        assert_eq!(report.deadlock_type, DeadlockType::None);
        assert!(!report.has_deadlock());
    }

    #[tokio::test]
    async fn cycle_detection_finds_deadlock() {
        let detector = DeadlockDetector::new();
        let a = detector
            .register_lock("A".into(), LockType::Mutex)
            .await
            .unwrap();
        let b = detector
            .register_lock("B".into(), LockType::Mutex)
            .await
            .unwrap();

        // Create cycle: A → B → A
        {
            let mut graph = detector.graph.write().await;
            graph.add_dependency(a, b);
            graph.add_dependency(b, a);
        }

        let report = detector.detect_deadlocks().await;
        assert_eq!(report.deadlock_type, DeadlockType::CycleDetected);
        assert!(report.has_deadlock());
        assert!(!report.involved_locks.is_empty());
    }

    #[tokio::test]
    async fn deadlock_report_no_deadlock() {
        let report = DeadlockReport {
            deadlock_type: DeadlockType::None,
            involved_locks: Vec::new(),
            lock_names: Vec::new(),
            prevention_strategy: String::new(),
            cycle_description: None,
        };
        assert!(!report.has_deadlock());
    }

    #[tokio::test]
    async fn get_deadlock_reports_empty() {
        let detector = DeadlockDetector::new();
        let reports = detector.deadlock_reports().await;
        assert!(reports.is_empty());
    }
}
