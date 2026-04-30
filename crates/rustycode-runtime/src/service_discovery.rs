//! Service Discovery and Registration System
//!
//! This module provides comprehensive service discovery with:
//! - Service registration and deregistration
//! - Health check integration
//! - Service discovery queries
//! - Load balancing strategies
//! - Service instance management
//! - Dynamic service updates
//! - Service dependency resolution

use crate::multi_agent::AgentRole;
use chrono::{DateTime, Utc};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Service instance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstance {
    pub id: String,
    pub service_name: String,
    pub host: String,
    pub port: u16,
    pub metadata: ServiceMetadata,
    pub capabilities: Vec<String>,
    pub tags: Vec<String>,
    pub version: String,
    pub registered_at: DateTime<Utc>,
    pub last_heartbeat: DateTime<Utc>,
    pub status: ServiceStatus,
    pub dependencies: Vec<String>, // Services this instance depends on
}

/// Service metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceMetadata {
    pub agent_role: Option<AgentRole>,
    pub weight: f64, // For load balancing
    pub priority: u32,
    pub region: Option<String>,
    pub zone: Option<String>,
    pub custom_data: HashMap<String, String>,
}

impl Default for ServiceMetadata {
    fn default() -> Self {
        Self {
            agent_role: None,
            weight: 1.0,
            priority: 0,
            region: None,
            zone: None,
            custom_data: HashMap::new(),
        }
    }
}

/// Service status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum ServiceStatus {
    Starting,
    Healthy,
    Unhealthy,
    Draining, // Gracefully shutting down
    Terminated,
}

impl ServiceStatus {
    pub fn is_available(&self) -> bool {
        matches!(self, ServiceStatus::Healthy | ServiceStatus::Draining)
    }
}

/// Service registry
#[derive(Debug, Clone)]
pub struct ServiceRegistry {
    pub service_name: String,
    pub instances: Vec<ServiceInstance>,
    pub strategy: LoadBalancingStrategy,
    pub created_at: DateTime<Utc>,
}

/// Load balancing strategies
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum LoadBalancingStrategy {
    RoundRobin,
    Random,
    LeastConnections,
    Weighted,
    Priority,
}

/// Service discovery query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceQuery {
    pub service_name: String,
    pub tags: Option<Vec<String>>,
    pub capabilities: Option<Vec<String>>,
    pub min_version: Option<String>,
    pub status: Option<ServiceStatus>,
    pub region: Option<String>,
}

/// Service registration request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRegistration {
    pub service_name: String,
    pub host: String,
    pub port: u16,
    pub metadata: ServiceMetadata,
    pub capabilities: Vec<String>,
    pub tags: Vec<String>,
    pub version: String,
    pub dependencies: Vec<String>,
    pub heartbeat_interval_seconds: u64,
}

/// Service discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDiscoveryConfig {
    pub heartbeat_timeout_seconds: u64,
    pub cleanup_interval_seconds: u64,
    pub enable_service_caching: bool,
    pub cache_ttl_seconds: u64,
    pub enable_auto_cleanup: bool,
}

impl Default for ServiceDiscoveryConfig {
    fn default() -> Self {
        Self {
            heartbeat_timeout_seconds: 30,
            cleanup_interval_seconds: 60,
            enable_service_caching: true,
            cache_ttl_seconds: 10,
            enable_auto_cleanup: true,
        }
    }
}

/// Main service discovery system
pub struct ServiceDiscovery {
    services: Arc<RwLock<HashMap<String, ServiceRegistry>>>,
    service_index: Arc<RwLock<HashMap<String, HashSet<String>>>>, // Tag/Service index
    instance_counter: Arc<RwLock<u64>>,
    config: ServiceDiscoveryConfig,
}

impl ServiceDiscovery {
    pub fn new(config: ServiceDiscoveryConfig) -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            service_index: Arc::new(RwLock::new(HashMap::new())),
            instance_counter: Arc::new(RwLock::new(0)),
            config,
        }
    }

    /// Register a service
    pub async fn register_service(
        &self,
        registration: ServiceRegistration,
    ) -> Result<String, String> {
        // Generate instance ID
        let mut counter = self.instance_counter.write().await;
        *counter += 1;
        let instance_id = format!("instance_{}", *counter);

        // Create service instance
        let instance = ServiceInstance {
            id: instance_id.clone(),
            service_name: registration.service_name.clone(),
            host: registration.host,
            port: registration.port,
            metadata: registration.metadata,
            capabilities: registration.capabilities,
            tags: registration.tags,
            version: registration.version,
            registered_at: Utc::now(),
            last_heartbeat: Utc::now(),
            status: ServiceStatus::Starting,
            dependencies: registration.dependencies,
        };

        // Add to registry
        {
            let mut services = self.services.write().await;

            let registry = services
                .entry(registration.service_name.clone())
                .or_insert_with(|| ServiceRegistry {
                    service_name: registration.service_name.clone(),
                    instances: Vec::new(),
                    strategy: LoadBalancingStrategy::RoundRobin,
                    created_at: Utc::now(),
                });

            registry.instances.push(instance.clone());
        }

        // Update index
        self.update_index(&instance).await;

        Ok(instance_id)
    }

    /// Deregister a service
    pub async fn deregister_service(&self, instance_id: &str) -> Result<(), String> {
        let removed_instance = {
            let mut services = self.services.write().await;

            // Find and remove instance
            let mut found = None;
            for (_, registry) in services.iter_mut() {
                if let Some(pos) = registry.instances.iter().position(|i| i.id == instance_id) {
                    found = Some(registry.instances.remove(pos));
                    break;
                }
            }
            // Drop the services lock before updating index to avoid holding two locks
            found
        };

        if let Some(instance) = removed_instance {
            self.remove_from_index(&instance).await;
            Ok(())
        } else {
            Err(format!("Service instance {} not found", instance_id))
        }
    }

    /// Send heartbeat
    pub async fn send_heartbeat(&self, instance_id: &str) -> Result<(), String> {
        let mut services = self.services.write().await;

        for registry in services.values_mut() {
            if let Some(instance) = registry.instances.iter_mut().find(|i| i.id == instance_id) {
                instance.last_heartbeat = Utc::now();

                // Update status based on heartbeat
                if instance.status == ServiceStatus::Starting {
                    instance.status = ServiceStatus::Healthy;
                }

                return Ok(());
            }
        }

        Err(format!("Service instance {} not found", instance_id))
    }

    /// Discover services
    pub async fn discover_services(&self, query: &ServiceQuery) -> Vec<ServiceInstance> {
        let services = self.services.read().await;

        if let Some(registry) = services.get(&query.service_name) {
            let mut instances = registry.instances.clone();

            // Filter by status
            if let Some(status) = query.status {
                instances.retain(|i| i.status == status);
            }

            // Filter by tags
            if let Some(ref tags) = query.tags {
                instances.retain(|i| tags.iter().all(|tag| i.tags.contains(tag)));
            }

            // Filter by capabilities
            if let Some(ref caps) = query.capabilities {
                instances.retain(|i| caps.iter().all(|cap| i.capabilities.contains(cap)));
            }

            // Filter by version
            if let Some(ref min_version) = query.min_version {
                instances.retain(|i| {
                    self.version_compare(&i.version, min_version) >= std::cmp::Ordering::Equal
                });
            }

            // Filter by region
            if let Some(ref region) = query.region {
                instances.retain(|i| i.metadata.region.as_ref().is_some_and(|r| r == region));
            }

            // Only return available instances
            instances.retain(|i| i.status.is_available());

            instances
        } else {
            Vec::new()
        }
    }

    /// Get service instance by ID
    pub async fn get_instance(&self, instance_id: &str) -> Option<ServiceInstance> {
        let services = self.services.read().await;

        for registry in services.values() {
            if let Some(instance) = registry.instances.iter().find(|i| i.id == instance_id) {
                return Some(instance.clone());
            }
        }

        None
    }

    /// Select instance using load balancing strategy
    pub async fn select_instance(
        &self,
        service_name: &str,
        strategy: Option<LoadBalancingStrategy>,
    ) -> Option<ServiceInstance> {
        let services = self.services.read().await;

        let registry = services.get(service_name)?;

        let available_instances: Vec<_> = registry
            .instances
            .iter()
            .filter(|i| i.status.is_available())
            .collect();

        if available_instances.is_empty() {
            return None;
        }

        let strategy = strategy.unwrap_or(registry.strategy);

        match strategy {
            LoadBalancingStrategy::RoundRobin => {
                // Use index based on count
                let count = available_instances.len();
                let index = {
                    let counter = self.instance_counter.read().await;
                    (*counter as usize) % count
                };
                available_instances
                    .get(index)
                    .map(|&instance| instance.clone())
            }
            LoadBalancingStrategy::Random => {
                let mut rng = rand::rng();
                let index = rng.random_range(0..available_instances.len());
                available_instances
                    .get(index)
                    .map(|&instance| instance.clone())
            }
            LoadBalancingStrategy::LeastConnections => {
                // Would need connection tracking - use first available
                available_instances
                    .first()
                    .map(|&instance| instance.clone())
            }
            LoadBalancingStrategy::Weighted => {
                // Select based on weight
                let total_weight: f64 = available_instances.iter().map(|i| i.metadata.weight).sum();

                let mut rng = rand::rng();
                let mut random_weight = rng.random_range(0.0..total_weight);

                for instance_ref in &available_instances {
                    random_weight -= instance_ref.metadata.weight;
                    if random_weight <= 0.0 {
                        return Some((*instance_ref).clone());
                    }
                }
                available_instances
                    .first()
                    .map(|&instance| instance.clone())
            }
            LoadBalancingStrategy::Priority => {
                // Sort by priority and return highest priority
                let mut instances: Vec<_> =
                    available_instances.iter().map(|&i| i.clone()).collect();
                instances.sort_by_key(|a| std::cmp::Reverse(a.metadata.priority));
                instances.into_iter().next()
            }
        }
    }

    /// Update service instance
    pub async fn update_instance(
        &self,
        instance_id: &str,
        updates: ServiceInstanceUpdate,
    ) -> Result<(), String> {
        let mut services = self.services.write().await;

        for registry in services.values_mut() {
            if let Some(instance) = registry.instances.iter_mut().find(|i| i.id == instance_id) {
                // Apply updates
                if let Some(status) = updates.status {
                    instance.status = status;
                }
                if let Some(capabilities) = updates.capabilities {
                    instance.capabilities = capabilities;
                }
                if let Some(tags) = updates.tags {
                    instance.tags = tags;
                }
                if let Some(metadata) = updates.metadata {
                    instance.metadata = metadata;
                }

                // Update index
                self.remove_from_index(instance).await;
                self.update_index(instance).await;

                return Ok(());
            }
        }

        Err(format!("Service instance {} not found", instance_id))
    }

    /// Get all services
    pub async fn get_all_services(&self) -> HashMap<String, ServiceRegistry> {
        let services = self.services.read().await;
        services.clone()
    }

    /// Get services by tag
    pub async fn get_services_by_tag(&self, tag: &str) -> Vec<String> {
        let index = self.service_index.read().await;
        index
            .get(tag)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect()
    }

    /// Check health of all instances
    pub async fn check_all_health(&self) -> HealthCheckResult {
        let services = self.services.read().await;
        let mut total = 0;
        let mut healthy = 0;
        let mut unhealthy = Vec::new();

        for (_service_name, registry) in services.iter() {
            for instance in &registry.instances {
                total += 1;

                let is_healthy = self.is_instance_healthy(instance).await;
                if is_healthy {
                    healthy += 1;
                } else {
                    unhealthy.push(instance.id.clone());
                }
            }
        }

        HealthCheckResult {
            total_instances: total,
            healthy_instances: healthy,
            unhealthy_instances: unhealthy,
            checked_at: Utc::now(),
        }
    }

    /// Clean up stale instances
    pub async fn cleanup_stale_instances(&self) -> Result<usize, String> {
        if !self.config.enable_auto_cleanup {
            return Ok(0);
        }

        let timeout = chrono::Duration::seconds(self.config.heartbeat_timeout_seconds as i64);
        let now = Utc::now();

        let mut stale_instances = Vec::new();

        {
            let services = self.services.read().await;
            for (service_name, registry) in services.iter() {
                for (idx, instance) in registry.instances.iter().enumerate() {
                    if now - instance.last_heartbeat > timeout {
                        stale_instances.push((service_name.clone(), idx));
                    }
                }
            }
        }

        // Remove stale instances
        let mut services = self.services.write().await;
        let mut cleaned = 0;

        for (service_name, instance_idx) in stale_instances {
            if let Some(registry) = services.get_mut(&service_name) {
                if instance_idx < registry.instances.len() {
                    let instance = registry.instances.remove(instance_idx);
                    self.remove_from_index(&instance).await;
                    cleaned += 1;
                }
            }
        }

        Ok(cleaned)
    }

    /// Update service index
    async fn update_index(&self, instance: &ServiceInstance) {
        let mut index = self.service_index.write().await;

        // Index by tags
        for tag in &instance.tags {
            index
                .entry(tag.clone())
                .or_insert_with(HashSet::new)
                .insert(instance.service_name.clone());
        }
    }

    /// Remove from index
    async fn remove_from_index(&self, instance: &ServiceInstance) {
        let mut index = self.service_index.write().await;

        // Remove from tags index
        for tag in &instance.tags {
            if let Some(service_names) = index.get_mut(tag) {
                service_names.remove(&instance.service_name);
                if service_names.is_empty() {
                    index.remove(tag);
                }
            }
        }
    }

    /// Check if instance is healthy
    async fn is_instance_healthy(&self, instance: &ServiceInstance) -> bool {
        // Check heartbeat
        let timeout = chrono::Duration::seconds(self.config.heartbeat_timeout_seconds as i64);
        if Utc::now() - instance.last_heartbeat > timeout {
            return false;
        }

        // Check status
        if !instance.status.is_available() {
            return false;
        }

        // Check dependencies
        if !instance.dependencies.is_empty() {
            let services = self.services.read().await;
            for dep_service in &instance.dependencies {
                if let Some(registry) = services.get(dep_service) {
                    let has_healthy = registry.instances.iter().any(|i| {
                        i.status.is_available() && Utc::now() - i.last_heartbeat <= timeout
                    });

                    if !has_healthy {
                        return false; // Dependency not healthy
                    }
                }
            }
        }

        true
    }

    /// Compare versions
    fn version_compare(&self, v1: &str, v2: &str) -> std::cmp::Ordering {
        // Simple semantic version comparison
        let v1_parts: Vec<u32> = v1.split('.').filter_map(|s| s.parse().ok()).collect();
        let v2_parts: Vec<u32> = v2.split('.').filter_map(|s| s.parse().ok()).collect();

        // Pad shorter version with zeros
        let max_len = v1_parts.len().max(v2_parts.len());
        let mut v1_padded = v1_parts.clone();
        let mut v2_padded = v2_parts.clone();

        v1_padded.resize(max_len, 0);
        v2_padded.resize(max_len, 0);

        v1_padded.iter().cmp(v2_padded.iter())
    }
}

/// Service instance update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstanceUpdate {
    pub status: Option<ServiceStatus>,
    pub capabilities: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<ServiceMetadata>,
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub total_instances: usize,
    pub healthy_instances: usize,
    pub unhealthy_instances: Vec<String>,
    pub checked_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_compare() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        assert_eq!(
            discovery.version_compare("1.0.0", "1.0.0"),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            discovery.version_compare("1.1.0", "1.0.0"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            discovery.version_compare("1.0.0", "1.1.0"),
            std::cmp::Ordering::Less
        );
    }

    #[tokio::test]
    async fn test_register_service() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let registration = ServiceRegistration {
            service_name: "test_service".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            metadata: ServiceMetadata {
                agent_role: Some(AgentRole::Reviewer),
                ..Default::default()
            },
            capabilities: vec!["read".to_string(), "write".to_string()],
            tags: vec!["api".to_string()],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let instance_id = discovery.register_service(registration).await.unwrap();
        assert!(instance_id.starts_with("instance_"));
    }

    #[tokio::test]
    async fn test_discover_services() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        // Register service
        let registration = ServiceRegistration {
            service_name: "test_service".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            metadata: ServiceMetadata::default(),
            capabilities: vec![],
            tags: vec!["api".to_string()],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let instance_id = discovery.register_service(registration).await.unwrap();
        discovery.send_heartbeat(&instance_id).await.unwrap();

        // Discover service
        let query = ServiceQuery {
            service_name: "test_service".to_string(),
            tags: Some(vec!["api".to_string()]),
            capabilities: None,
            min_version: None,
            status: None,
            region: None,
        };

        let instances = discovery.discover_services(&query).await;
        assert_eq!(instances.len(), 1);
    }

    #[tokio::test]
    async fn test_heartbeat() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let registration = ServiceRegistration {
            service_name: "test_service".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            metadata: ServiceMetadata::default(),
            capabilities: vec![],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let instance_id = discovery.register_service(registration).await.unwrap();
        discovery.send_heartbeat(&instance_id).await.unwrap();

        let instance = discovery.get_instance(&instance_id).await.unwrap();
        assert_eq!(instance.status, ServiceStatus::Healthy);
    }

    #[tokio::test]
    async fn test_deregister_service() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let registration = ServiceRegistration {
            service_name: "test_service".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            metadata: ServiceMetadata::default(),
            capabilities: vec![],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let instance_id = discovery.register_service(registration).await.unwrap();
        discovery.deregister_service(&instance_id).await.unwrap();

        let instance = discovery.get_instance(&instance_id).await;
        assert!(instance.is_none());
    }

    #[tokio::test]
    async fn test_health_check() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let registration = ServiceRegistration {
            service_name: "test_service".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            metadata: ServiceMetadata::default(),
            capabilities: vec![],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        discovery.register_service(registration).await.unwrap();

        let health = discovery.check_all_health().await;
        assert_eq!(health.total_instances, 1);
        assert_eq!(health.healthy_instances, 0); // Starting state not counted as healthy yet
    }

    // --- New tests below ---

    #[test]
    fn test_service_status_is_available() {
        assert!(!ServiceStatus::Starting.is_available());
        assert!(ServiceStatus::Healthy.is_available());
        assert!(!ServiceStatus::Unhealthy.is_available());
        assert!(ServiceStatus::Draining.is_available());
        assert!(!ServiceStatus::Terminated.is_available());
    }

    #[test]
    fn test_service_metadata_default() {
        let meta = ServiceMetadata::default();
        assert!(meta.agent_role.is_none());
        assert_eq!(meta.weight, 1.0);
        assert_eq!(meta.priority, 0);
        assert!(meta.region.is_none());
        assert!(meta.zone.is_none());
        assert!(meta.custom_data.is_empty());
    }

    #[test]
    fn test_service_discovery_config_default() {
        let config = ServiceDiscoveryConfig::default();
        assert_eq!(config.heartbeat_timeout_seconds, 30);
        assert_eq!(config.cleanup_interval_seconds, 60);
        assert!(config.enable_service_caching);
        assert_eq!(config.cache_ttl_seconds, 10);
        assert!(config.enable_auto_cleanup);
    }

    #[test]
    fn test_load_balancing_strategy_serialization() {
        for strategy in [
            LoadBalancingStrategy::RoundRobin,
            LoadBalancingStrategy::Random,
            LoadBalancingStrategy::LeastConnections,
            LoadBalancingStrategy::Weighted,
            LoadBalancingStrategy::Priority,
        ] {
            let json = serde_json::to_string(&strategy).unwrap();
            let back: LoadBalancingStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(back, strategy);
        }
    }

    #[tokio::test]
    async fn test_register_multiple_services_and_get_all() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let reg1 = ServiceRegistration {
            service_name: "svc_alpha".to_string(),
            host: "host1".to_string(),
            port: 8001,
            metadata: ServiceMetadata::default(),
            capabilities: vec![],
            tags: vec!["web".to_string()],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let reg2 = ServiceRegistration {
            service_name: "svc_beta".to_string(),
            host: "host2".to_string(),
            port: 8002,
            metadata: ServiceMetadata::default(),
            capabilities: vec![],
            tags: vec!["api".to_string()],
            version: "2.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        discovery.register_service(reg1).await.unwrap();
        discovery.register_service(reg2).await.unwrap();

        let all = discovery.get_all_services().await;
        assert_eq!(all.len(), 2);
        assert!(all.contains_key("svc_alpha"));
        assert!(all.contains_key("svc_beta"));
    }

    #[tokio::test]
    async fn test_deregister_nonexistent() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());
        let result = discovery.deregister_service("nonexistent").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_send_heartbeat_nonexistent() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());
        let result = discovery.send_heartbeat("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_heartbeat_transitions_starting_to_healthy() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let reg = ServiceRegistration {
            service_name: "test".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            metadata: ServiceMetadata::default(),
            capabilities: vec![],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let id = discovery.register_service(reg).await.unwrap();
        let instance = discovery.get_instance(&id).await.unwrap();
        assert_eq!(instance.status, ServiceStatus::Starting);

        discovery.send_heartbeat(&id).await.unwrap();
        let instance = discovery.get_instance(&id).await.unwrap();
        assert_eq!(instance.status, ServiceStatus::Healthy);
    }

    #[tokio::test]
    async fn test_discover_with_capability_filter() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let reg = ServiceRegistration {
            service_name: "worker".to_string(),
            host: "localhost".to_string(),
            port: 9000,
            metadata: ServiceMetadata::default(),
            capabilities: vec!["read".to_string(), "write".to_string()],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let id = discovery.register_service(reg).await.unwrap();
        discovery.send_heartbeat(&id).await.unwrap();

        let query = ServiceQuery {
            service_name: "worker".to_string(),
            tags: None,
            capabilities: Some(vec!["read".to_string()]),
            min_version: None,
            status: None,
            region: None,
        };
        let results = discovery.discover_services(&query).await;
        assert_eq!(results.len(), 1);

        let query_missing_cap = ServiceQuery {
            service_name: "worker".to_string(),
            tags: None,
            capabilities: Some(vec!["execute".to_string()]),
            min_version: None,
            status: None,
            region: None,
        };
        let results = discovery.discover_services(&query_missing_cap).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_discover_with_version_filter() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let reg = ServiceRegistration {
            service_name: "api".to_string(),
            host: "localhost".to_string(),
            port: 9000,
            metadata: ServiceMetadata::default(),
            capabilities: vec![],
            tags: vec![],
            version: "2.5.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let id = discovery.register_service(reg).await.unwrap();
        discovery.send_heartbeat(&id).await.unwrap();

        // Min version 2.0.0 should match 2.5.0
        let query_match = ServiceQuery {
            service_name: "api".to_string(),
            tags: None,
            capabilities: None,
            min_version: Some("2.0.0".to_string()),
            status: None,
            region: None,
        };
        let results = discovery.discover_services(&query_match).await;
        assert_eq!(results.len(), 1);

        // Min version 3.0.0 should NOT match 2.5.0
        let query_no_match = ServiceQuery {
            service_name: "api".to_string(),
            tags: None,
            capabilities: None,
            min_version: Some("3.0.0".to_string()),
            status: None,
            region: None,
        };
        let results = discovery.discover_services(&query_no_match).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_get_services_by_tag() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let reg = ServiceRegistration {
            service_name: "svc_a".to_string(),
            host: "localhost".to_string(),
            port: 8001,
            metadata: ServiceMetadata::default(),
            capabilities: vec![],
            tags: vec!["web".to_string(), "public".to_string()],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        discovery.register_service(reg).await.unwrap();

        let web_services = discovery.get_services_by_tag("web").await;
        assert_eq!(web_services, vec!["svc_a"]);

        let public_services = discovery.get_services_by_tag("public").await;
        assert_eq!(public_services, vec!["svc_a"]);

        let unknown = discovery.get_services_by_tag("unknown").await;
        assert!(unknown.is_empty());
    }

    #[tokio::test]
    async fn test_update_instance() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let reg = ServiceRegistration {
            service_name: "svc".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            metadata: ServiceMetadata::default(),
            capabilities: vec!["read".to_string()],
            tags: vec!["v1".to_string()],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let id = discovery.register_service(reg).await.unwrap();

        let update = ServiceInstanceUpdate {
            status: Some(ServiceStatus::Healthy),
            capabilities: Some(vec!["read".to_string(), "write".to_string()]),
            tags: Some(vec!["v2".to_string()]),
            metadata: None,
        };

        discovery.update_instance(&id, update).await.unwrap();

        let instance = discovery.get_instance(&id).await.unwrap();
        assert_eq!(instance.status, ServiceStatus::Healthy);
        assert_eq!(instance.capabilities, vec!["read", "write"]);
        assert_eq!(instance.tags, vec!["v2"]);
    }

    #[tokio::test]
    async fn test_update_nonexistent_instance() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let update = ServiceInstanceUpdate {
            status: Some(ServiceStatus::Healthy),
            capabilities: None,
            tags: None,
            metadata: None,
        };

        let result = discovery.update_instance("nonexistent", update).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_select_instance_priority_strategy() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        // Register two instances with different priorities
        let reg1 = ServiceRegistration {
            service_name: "svc".to_string(),
            host: "host1".to_string(),
            port: 8001,
            metadata: ServiceMetadata {
                priority: 5,
                ..Default::default()
            },
            capabilities: vec![],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let reg2 = ServiceRegistration {
            service_name: "svc".to_string(),
            host: "host2".to_string(),
            port: 8002,
            metadata: ServiceMetadata {
                priority: 10,
                ..Default::default()
            },
            capabilities: vec![],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let id1 = discovery.register_service(reg1).await.unwrap();
        let id2 = discovery.register_service(reg2).await.unwrap();
        discovery.send_heartbeat(&id1).await.unwrap();
        discovery.send_heartbeat(&id2).await.unwrap();

        let selected = discovery
            .select_instance("svc", Some(LoadBalancingStrategy::Priority))
            .await;
        assert!(selected.is_some());
        // Priority strategy should select the instance with priority 10
        assert_eq!(selected.unwrap().metadata.priority, 10);
    }

    #[test]
    fn test_version_compare_edge_cases() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        // Different lengths
        assert_eq!(
            discovery.version_compare("1.0", "1.0.0"),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            discovery.version_compare("2.0", "1.9.9"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            discovery.version_compare("0.1.0", "0.2.0"),
            std::cmp::Ordering::Less
        );

        // Single component
        assert_eq!(
            discovery.version_compare("5", "3"),
            std::cmp::Ordering::Greater
        );
    }

    #[tokio::test]
    async fn test_service_registration_serialization() {
        let reg = ServiceRegistration {
            service_name: "test".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            metadata: ServiceMetadata {
                agent_role: Some(AgentRole::Reviewer),
                weight: 2.5,
                priority: 3,
                region: Some("us-east".to_string()),
                zone: Some("a".to_string()),
                custom_data: HashMap::from([("env".to_string(), "prod".to_string())]),
            },
            capabilities: vec!["read".to_string()],
            tags: vec!["api".to_string()],
            version: "1.0.0".to_string(),
            dependencies: vec!["db".to_string()],
            heartbeat_interval_seconds: 15,
        };

        let json = serde_json::to_string(&reg).unwrap();
        let back: ServiceRegistration = serde_json::from_str(&json).unwrap();
        assert_eq!(back.service_name, "test");
        assert_eq!(back.port, 8080);
        assert_eq!(back.metadata.weight, 2.5);
        assert_eq!(back.dependencies, vec!["db"]);
    }

    #[tokio::test]
    async fn test_health_check_result_fields() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let reg = ServiceRegistration {
            service_name: "svc".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            metadata: ServiceMetadata::default(),
            capabilities: vec![],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };
        discovery.register_service(reg).await.unwrap();

        let health = discovery.check_all_health().await;
        assert_eq!(health.total_instances, 1);
        // Starting status means not healthy (no heartbeat yet)
        assert_eq!(health.healthy_instances, 0);
        assert_eq!(health.unhealthy_instances.len(), 1);
    }

    #[tokio::test]
    async fn test_get_instance_nonexistent() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());
        let result = discovery.get_instance("nonexistent").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_select_instance_no_available() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        // Register but don't heartbeat -- status stays Starting, not available
        let reg = ServiceRegistration {
            service_name: "svc".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            metadata: ServiceMetadata::default(),
            capabilities: vec![],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };
        discovery.register_service(reg).await.unwrap();

        let selected = discovery.select_instance("svc", None).await;
        assert!(selected.is_none());
    }

    // --- 15 additional terminal-bench tests ---

    // Validates that ServiceMetadata serializes and deserializes correctly
    // including all optional fields and the custom_data map.
    #[test]
    fn test_service_metadata_serde_roundtrip() {
        let meta = ServiceMetadata {
            agent_role: Some(AgentRole::Reviewer),
            weight: 3.7,
            priority: 42,
            region: Some("eu-west".to_string()),
            zone: Some("b".to_string()),
            custom_data: HashMap::from([
                ("env".to_string(), "staging".to_string()),
                ("team".to_string(), "platform".to_string()),
            ]),
        };

        let json = serde_json::to_string(&meta).unwrap();
        let back: ServiceMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(back.agent_role, meta.agent_role);
        assert_eq!(back.weight, meta.weight);
        assert_eq!(back.priority, meta.priority);
        assert_eq!(back.region, meta.region);
        assert_eq!(back.zone, meta.zone);
        assert_eq!(back.custom_data, meta.custom_data);
    }

    // Validates that ServiceStatus variants serialize to their expected
    // string representations and roundtrip through JSON correctly.
    #[test]
    fn test_service_status_serde_roundtrip() {
        for status in [
            ServiceStatus::Starting,
            ServiceStatus::Healthy,
            ServiceStatus::Unhealthy,
            ServiceStatus::Draining,
            ServiceStatus::Terminated,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let back: ServiceStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(back, status);
        }
    }

    // Validates that HealthCheckResult serializes and deserializes with
    // its DateTime field intact and the unhealthy_instances list preserved.
    #[test]
    fn test_health_check_result_serde_roundtrip() {
        let result = HealthCheckResult {
            total_instances: 5,
            healthy_instances: 3,
            unhealthy_instances: vec!["inst_1".to_string(), "inst_4".to_string()],
            checked_at: Utc::now(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let back: HealthCheckResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.total_instances, 5);
        assert_eq!(back.healthy_instances, 3);
        assert_eq!(back.unhealthy_instances, vec!["inst_1", "inst_4"]);
    }

    // Validates that ServiceInstanceUpdate with all fields set to None
    // still serializes/deserializes correctly (nullable optional fields).
    #[test]
    fn test_service_instance_update_serde_all_none() {
        let update = ServiceInstanceUpdate {
            status: None,
            capabilities: None,
            tags: None,
            metadata: None,
        };

        let json = serde_json::to_string(&update).unwrap();
        let back: ServiceInstanceUpdate = serde_json::from_str(&json).unwrap();
        assert!(back.status.is_none());
        assert!(back.capabilities.is_none());
        assert!(back.tags.is_none());
        assert!(back.metadata.is_none());
    }

    // Validates that ServiceQuery with all optional filters active
    // roundtrips through JSON without losing any filter criteria.
    #[test]
    fn test_service_query_serde_full_filters() {
        let query = ServiceQuery {
            service_name: "search_svc".to_string(),
            tags: Some(vec!["web".to_string()]),
            capabilities: Some(vec!["index".to_string(), "query".to_string()]),
            min_version: Some("2.0.0".to_string()),
            status: Some(ServiceStatus::Healthy),
            region: Some("us-west".to_string()),
        };

        let json = serde_json::to_string(&query).unwrap();
        let back: ServiceQuery = serde_json::from_str(&json).unwrap();
        assert_eq!(back.service_name, "search_svc");
        assert_eq!(back.tags.as_ref().unwrap().len(), 1);
        assert_eq!(back.capabilities.as_ref().unwrap().len(), 2);
        assert_eq!(back.min_version.as_ref().unwrap(), "2.0.0");
        assert_eq!(back.status.unwrap(), ServiceStatus::Healthy);
        assert_eq!(back.region.as_ref().unwrap(), "us-west");
    }

    // Validates that discover_services filters by region, excluding instances
    // whose metadata region does not match the query region.
    #[tokio::test]
    async fn test_discover_with_region_filter() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let reg_east = ServiceRegistration {
            service_name: "geo_svc".to_string(),
            host: "east-host".to_string(),
            port: 8001,
            metadata: ServiceMetadata {
                region: Some("us-east".to_string()),
                ..Default::default()
            },
            capabilities: vec![],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let reg_west = ServiceRegistration {
            service_name: "geo_svc".to_string(),
            host: "west-host".to_string(),
            port: 8002,
            metadata: ServiceMetadata {
                region: Some("us-west".to_string()),
                ..Default::default()
            },
            capabilities: vec![],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let id_east = discovery.register_service(reg_east).await.unwrap();
        let id_west = discovery.register_service(reg_west).await.unwrap();
        discovery.send_heartbeat(&id_east).await.unwrap();
        discovery.send_heartbeat(&id_west).await.unwrap();

        let query = ServiceQuery {
            service_name: "geo_svc".to_string(),
            tags: None,
            capabilities: None,
            min_version: None,
            status: None,
            region: Some("us-west".to_string()),
        };

        let results = discovery.discover_services(&query).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].host, "west-host");
    }

    // Validates that deregistering an instance removes it from the tag index,
    // so subsequent get_services_by_tag calls no longer return that service.
    #[tokio::test]
    async fn test_tag_index_cleanup_on_deregister() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let reg = ServiceRegistration {
            service_name: "tagged_svc".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            metadata: ServiceMetadata::default(),
            capabilities: vec![],
            tags: vec!["solo".to_string()],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let id = discovery.register_service(reg).await.unwrap();

        // Service should appear under the "solo" tag
        let tagged = discovery.get_services_by_tag("solo").await;
        assert_eq!(tagged, vec!["tagged_svc"]);

        // After deregister, the tag index should be cleaned up
        discovery.deregister_service(&id).await.unwrap();
        let tagged_after = discovery.get_services_by_tag("solo").await;
        assert!(tagged_after.is_empty());
    }

    // Validates that multiple instances can be registered under the same
    // service name and are all returned when querying that service.
    #[tokio::test]
    async fn test_multiple_instances_per_service() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        for port in [9001, 9002, 9003] {
            let reg = ServiceRegistration {
                service_name: "multi_svc".to_string(),
                host: "localhost".to_string(),
                port,
                metadata: ServiceMetadata::default(),
                capabilities: vec![],
                tags: vec![],
                version: "1.0.0".to_string(),
                dependencies: vec![],
                heartbeat_interval_seconds: 30,
            };
            let id = discovery.register_service(reg).await.unwrap();
            discovery.send_heartbeat(&id).await.unwrap();
        }

        let all = discovery.get_all_services().await;
        let registry = all.get("multi_svc").unwrap();
        assert_eq!(registry.instances.len(), 3);

        let query = ServiceQuery {
            service_name: "multi_svc".to_string(),
            tags: None,
            capabilities: None,
            min_version: None,
            status: None,
            region: None,
        };
        let instances = discovery.discover_services(&query).await;
        assert_eq!(instances.len(), 3);
    }

    // Validates that instances with Draining status are considered available
    // and appear in discovery results, as Draining is a graceful-shutdown state.
    #[tokio::test]
    async fn test_draining_status_is_available_in_discovery() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let reg = ServiceRegistration {
            service_name: "drain_svc".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            metadata: ServiceMetadata::default(),
            capabilities: vec![],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let id = discovery.register_service(reg).await.unwrap();
        discovery.send_heartbeat(&id).await.unwrap();

        // Transition to Draining
        let update = ServiceInstanceUpdate {
            status: Some(ServiceStatus::Draining),
            capabilities: None,
            tags: None,
            metadata: None,
        };
        discovery.update_instance(&id, update).await.unwrap();

        let query = ServiceQuery {
            service_name: "drain_svc".to_string(),
            tags: None,
            capabilities: None,
            min_version: None,
            status: None,
            region: None,
        };

        let results = discovery.discover_services(&query).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, ServiceStatus::Draining);
    }

    // Validates that cleanup_stale_instances returns 0 when auto cleanup
    // is disabled in the config, regardless of heartbeat state.
    #[tokio::test]
    async fn test_cleanup_disabled_by_config() {
        let config = ServiceDiscoveryConfig {
            enable_auto_cleanup: false,
            ..Default::default()
        };
        let discovery = ServiceDiscovery::new(config);

        let reg = ServiceRegistration {
            service_name: "stale_svc".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            metadata: ServiceMetadata::default(),
            capabilities: vec![],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };
        discovery.register_service(reg).await.unwrap();

        let cleaned = discovery.cleanup_stale_instances().await.unwrap();
        assert_eq!(cleaned, 0);
    }

    // Validates that the Weighted load balancing strategy returns a valid
    // instance and never panics even when all weights are equal.
    #[tokio::test]
    async fn test_select_instance_weighted_strategy() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let reg1 = ServiceRegistration {
            service_name: "w_svc".to_string(),
            host: "host1".to_string(),
            port: 8001,
            metadata: ServiceMetadata {
                weight: 1.0,
                ..Default::default()
            },
            capabilities: vec![],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let reg2 = ServiceRegistration {
            service_name: "w_svc".to_string(),
            host: "host2".to_string(),
            port: 8002,
            metadata: ServiceMetadata {
                weight: 4.0,
                ..Default::default()
            },
            capabilities: vec![],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let id1 = discovery.register_service(reg1).await.unwrap();
        let id2 = discovery.register_service(reg2).await.unwrap();
        discovery.send_heartbeat(&id1).await.unwrap();
        discovery.send_heartbeat(&id2).await.unwrap();

        // Run weighted selection multiple times -- it should always return Some
        for _ in 0..20 {
            let selected = discovery
                .select_instance("w_svc", Some(LoadBalancingStrategy::Weighted))
                .await;
            assert!(selected.is_some());
        }
    }

    // Validates that the LeastConnections strategy returns the first available
    // instance when no connection tracking is in place (current implementation).
    #[tokio::test]
    async fn test_select_instance_least_connections_strategy() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let reg = ServiceRegistration {
            service_name: "lc_svc".to_string(),
            host: "lc-host".to_string(),
            port: 8080,
            metadata: ServiceMetadata::default(),
            capabilities: vec![],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let id = discovery.register_service(reg).await.unwrap();
        discovery.send_heartbeat(&id).await.unwrap();

        let selected = discovery
            .select_instance("lc_svc", Some(LoadBalancingStrategy::LeastConnections))
            .await;
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().host, "lc-host");
    }

    // Validates that select_instance returns None for a service name
    // that has never been registered.
    #[tokio::test]
    async fn test_select_instance_unknown_service() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let selected = discovery.select_instance("no_such_service", None).await;
        assert!(selected.is_none());
    }

    // Validates that update_instance can replace the entire metadata,
    // including the custom_data map, weight, and priority fields.
    #[tokio::test]
    async fn test_update_instance_replaces_metadata() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let reg = ServiceRegistration {
            service_name: "meta_svc".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            metadata: ServiceMetadata {
                weight: 1.0,
                priority: 0,
                custom_data: HashMap::new(),
                ..Default::default()
            },
            capabilities: vec![],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let id = discovery.register_service(reg).await.unwrap();

        let new_metadata = ServiceMetadata {
            agent_role: Some(AgentRole::Reviewer),
            weight: 5.0,
            priority: 100,
            region: Some("ap-south".to_string()),
            zone: Some("c".to_string()),
            custom_data: HashMap::from([("deployed_by".to_string(), "ci".to_string())]),
        };

        let update = ServiceInstanceUpdate {
            status: None,
            capabilities: None,
            tags: None,
            metadata: Some(new_metadata.clone()),
        };

        discovery.update_instance(&id, update).await.unwrap();

        let instance = discovery.get_instance(&id).await.unwrap();
        assert_eq!(instance.metadata.weight, 5.0);
        assert_eq!(instance.metadata.priority, 100);
        assert_eq!(instance.metadata.region.as_ref().unwrap(), "ap-south");
        assert_eq!(
            instance.metadata.custom_data.get("deployed_by").unwrap(),
            "ci"
        );
    }

    // Validates that after heartbeating an instance, the health check
    // reports it as healthy (not in the unhealthy list).
    #[tokio::test]
    async fn test_health_check_after_heartbeat() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let reg = ServiceRegistration {
            service_name: "hb_svc".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            metadata: ServiceMetadata::default(),
            capabilities: vec![],
            tags: vec![],
            version: "1.0.0".to_string(),
            dependencies: vec![],
            heartbeat_interval_seconds: 30,
        };

        let id = discovery.register_service(reg).await.unwrap();

        // Before heartbeat -- Starting, not healthy
        let health_before = discovery.check_all_health().await;
        assert_eq!(health_before.healthy_instances, 0);

        discovery.send_heartbeat(&id).await.unwrap();

        // After heartbeat -- Healthy
        let health_after = discovery.check_all_health().await;
        assert_eq!(health_after.healthy_instances, 1);
        assert!(health_after.unhealthy_instances.is_empty());
    }

    // Validates that discover_services returns an empty vec when querying
    // a service name that does not exist in the registry.
    #[tokio::test]
    async fn test_discover_nonexistent_service_returns_empty() {
        let discovery = ServiceDiscovery::new(ServiceDiscoveryConfig::default());

        let query = ServiceQuery {
            service_name: "ghost_svc".to_string(),
            tags: None,
            capabilities: None,
            min_version: None,
            status: None,
            region: None,
        };

        let results = discovery.discover_services(&query).await;
        assert!(results.is_empty());
    }
}
