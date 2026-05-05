//! Resource Allocation and Management System
//!
//! This module provides comprehensive resource management with:
//! - Dynamic resource allocation across agents
//! - Cost optimization and budget management
//! - Capacity planning and forecasting
//! - Resource pool management
//! - Real-time utilization tracking
//! - Auto-scaling capabilities

use crate::multi_agent::AgentRole;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;

/// Resource types that can be allocated
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ResourceType {
    Cpu,
    Memory,
    Gpu,
    Network,
    Storage,
    ApiQuota,
}

/// Resource pool for a specific type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePool {
    pub resource_type: ResourceType,
    pub total_capacity: f64,
    pub allocated: f64,
    pub reserved: f64,
    pub available: f64,
    pub utilization_percent: f64,
    pub last_updated: DateTime<Utc>,
}

impl ResourcePool {
    /// Calculate current utilization
    pub fn calculate_utilization(&mut self) {
        self.utilization_percent = if self.total_capacity > 0.0 {
            (self.allocated / self.total_capacity) * 100.0
        } else {
            0.0
        };
        self.available = self.total_capacity - self.allocated - self.reserved;
        self.last_updated = Utc::now();
    }

    /// Check if capacity is available
    pub fn has_capacity(&self, amount: f64) -> bool {
        self.available >= amount
    }

    /// Reserve resources
    pub fn reserve(&mut self, amount: f64) -> Result<(), String> {
        if self.has_capacity(amount) {
            self.reserved += amount;
            self.calculate_utilization();
            Ok(())
        } else {
            Err(format!(
                "Insufficient capacity: requested {}, available {}",
                amount, self.available
            ))
        }
    }

    /// Allocate reserved resources
    pub fn allocate(&mut self, amount: f64) -> Result<(), String> {
        if self.reserved >= amount {
            self.reserved -= amount;
            self.allocated += amount;
            self.calculate_utilization();
            Ok(())
        } else {
            Err(format!(
                "Cannot allocate more than reserved: requested {}, reserved {}",
                amount, self.reserved
            ))
        }
    }

    /// Release resources back to pool
    pub fn release(&mut self, amount: f64) -> Result<(), String> {
        if self.allocated >= amount {
            self.allocated -= amount;
            self.calculate_utilization();
            Ok(())
        } else {
            Err(format!(
                "Cannot release more than allocated: requested {}, allocated {}",
                amount, self.allocated
            ))
        }
    }
}

/// Resource allocation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequest {
    pub request_id: String,
    pub agent_role: AgentRole,
    pub task_id: String,
    pub resources: HashMap<ResourceType, f64>,
    pub priority: AllocationPriority,
    pub max_cost: f64,
    pub duration_estimate_ms: u64,
    pub created_at: DateTime<Utc>,
    pub deadline: Option<DateTime<Utc>>,
}

/// Allocation priority levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum AllocationPriority {
    Critical = 5,
    High = 4,
    Medium = 3,
    Low = 2,
    Background = 1,
}

/// Resource allocation decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocationDecision {
    pub request_id: String,
    pub approved: bool,
    pub allocated_resources: HashMap<ResourceType, f64>,
    pub estimated_cost: f64,
    pub reason: String,
    pub confidence: f64,
    pub alternative_suggestions: Vec<String>,
    pub allocated_at: DateTime<Utc>,
}

/// Cost optimization settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostOptimization {
    pub hourly_cost_cpu: f64,
    pub hourly_cost_memory_gb: f64,
    pub hourly_cost_gpu: f64,
    pub hourly_cost_network_gb: f64,
    pub hourly_cost_storage_gb: f64,
    pub budget_limit_hourly: f64,
    pub budget_limit_daily: f64,
    pub enable_auto_scaling: bool,
    pub target_utilization_percent: f64,
}

impl Default for CostOptimization {
    fn default() -> Self {
        Self {
            hourly_cost_cpu: 0.05,
            hourly_cost_memory_gb: 0.01,
            hourly_cost_gpu: 1.0,
            hourly_cost_network_gb: 0.01,
            hourly_cost_storage_gb: 0.001,
            budget_limit_hourly: 10.0,
            budget_limit_daily: 100.0,
            enable_auto_scaling: true,
            target_utilization_percent: 70.0,
        }
    }
}

/// Capacity forecast
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityForecast {
    pub forecast_date: DateTime<Utc>,
    pub predicted_cpu_utilization: f64,
    pub predicted_memory_utilization: f64,
    pub recommended_scale_action: ScaleAction,
    pub confidence: f64,
}

/// Scaling actions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum ScaleAction {
    ScaleUp {
        resources: HashMap<ResourceType, f64>,
    },
    ScaleDown {
        resources: HashMap<ResourceType, f64>,
    },
    NoAction,
    ScaleUpRestricted {
        reason: String,
    },
    ScaleDownRestricted {
        reason: String,
    },
}

/// Resource utilization metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtilizationMetrics {
    pub timestamp: DateTime<Utc>,
    pub cpu_utilization_percent: f64,
    pub memory_utilization_percent: f64,
    pub gpu_utilization_percent: f64,
    pub network_utilization_mb: f64,
    pub active_allocations: usize,
    pub pending_requests: usize,
    pub total_cost_hourly: f64,
}

/// Resource manager configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceManagerConfig {
    pub enable_cost_optimization: bool,
    pub enable_capacity_planning: bool,
    pub enable_auto_scaling: bool,
    pub forecasting_horizon_hours: u64,
    pub utilization_history_size: usize,
    pub allocation_timeout_ms: u64,
}

impl Default for ResourceManagerConfig {
    fn default() -> Self {
        Self {
            enable_cost_optimization: true,
            enable_capacity_planning: true,
            enable_auto_scaling: true,
            forecasting_horizon_hours: 24,
            utilization_history_size: 1000,
            allocation_timeout_ms: 5000,
        }
    }
}

/// Main resource manager
pub struct ResourceManager {
    resource_pools: Arc<RwLock<HashMap<ResourceType, ResourcePool>>>,
    active_allocations: Arc<RwLock<HashMap<String, ResourceRequest>>>,
    allocation_history: Arc<RwLock<Vec<AllocationDecision>>>,
    utilization_metrics: Arc<RwLock<Vec<UtilizationMetrics>>>,
    cost_settings: Arc<RwLock<CostOptimization>>,
    config: ResourceManagerConfig,
    request_counter: Arc<RwLock<u64>>,
}

impl ResourceManager {
    pub fn new(initial_pools: HashMap<ResourceType, f64>, config: ResourceManagerConfig) -> Self {
        let pools = initial_pools
            .into_iter()
            .map(|(resource_type, capacity)| {
                let pool = ResourcePool {
                    resource_type,
                    total_capacity: capacity,
                    allocated: 0.0,
                    reserved: 0.0,
                    available: capacity,
                    utilization_percent: 0.0,
                    last_updated: Utc::now(),
                };
                (resource_type, pool)
            })
            .collect();

        Self {
            resource_pools: Arc::new(RwLock::new(pools)),
            active_allocations: Arc::new(RwLock::new(HashMap::new())),
            allocation_history: Arc::new(RwLock::new(Vec::new())),
            utilization_metrics: Arc::new(RwLock::new(Vec::new())),
            cost_settings: Arc::new(RwLock::new(CostOptimization::default())),
            config,
            request_counter: Arc::new(RwLock::new(0)),
        }
    }

    /// Request resource allocation
    pub async fn request_resources(
        &self,
        mut request: ResourceRequest,
    ) -> Result<AllocationDecision, String> {
        // Generate request ID if not provided
        if request.request_id.is_empty() {
            let mut counter = self.request_counter.write().await;
            *counter += 1;
            request.request_id = format!("req_{}", *counter);
        }

        // Check if resources are available
        let mut pools = self.resource_pools.write().await;
        let mut allocated = HashMap::new();
        let mut reasons = Vec::new();
        let mut can_fulfill = true;

        for (resource_type, amount) in &request.resources {
            if let Some(pool) = pools.get_mut(resource_type) {
                if pool.has_capacity(*amount) {
                    pool.reserve(*amount)?;
                    allocated.insert(*resource_type, *amount);
                    reasons.push(format!(
                        "{:?}: {:.2} available",
                        resource_type, pool.available
                    ));
                } else {
                    can_fulfill = false;
                    reasons.push(format!(
                        "{:?}: insufficient capacity (need {}, have {})",
                        resource_type, amount, pool.available
                    ));
                }
            } else {
                can_fulfill = false;
                reasons.push(format!("{:?}: resource pool not found", resource_type));
            }
        }

        // Calculate estimated cost
        let estimated_cost = self.calculate_cost(&allocated, &request).await?;

        // Check budget constraints
        let _cost_settings = self.cost_settings.read().await;
        if estimated_cost > request.max_cost {
            can_fulfill = false;
            reasons.push(format!(
                "Cost {:.2} exceeds max budget {:.2}",
                estimated_cost, request.max_cost
            ));
        }

        // Generate decision
        let decision = if can_fulfill {
            // Allocate reserved resources
            for (resource_type, amount) in &allocated {
                if let Some(pool) = pools.get_mut(resource_type) {
                    pool.allocate(*amount)?;
                }
            }

            AllocationDecision {
                request_id: request.request_id.clone(),
                approved: true,
                allocated_resources: allocated.clone(),
                estimated_cost,
                reason: reasons.join("; "),
                confidence: self
                    .calculate_allocation_confidence(&request, &allocated)
                    .await,
                alternative_suggestions: Vec::new(),
                allocated_at: Utc::now(),
            }
        } else {
            // Release reservations
            for resource_type in allocated.keys() {
                if let Some(pool) = pools.get_mut(resource_type) {
                    if let Err(e) = pool.release(*allocated.get(resource_type).unwrap_or(&0.0)) {
                        warn!("Failed to release {:?} resources: {}", resource_type, e);
                    }
                }
            }

            let alternatives = self.generate_alternatives(&request, &pools).await;

            AllocationDecision {
                request_id: request.request_id.clone(),
                approved: false,
                allocated_resources: HashMap::new(),
                estimated_cost: 0.0,
                reason: reasons.join("; "),
                confidence: 0.0,
                alternative_suggestions: alternatives,
                allocated_at: Utc::now(),
            }
        };

        // Store allocation and history
        if decision.approved {
            let mut active = self.active_allocations.write().await;
            active.insert(request.request_id.clone(), request);
        }

        let mut history = self.allocation_history.write().await;
        history.push(decision.clone());

        Ok(decision)
    }

    /// Release allocated resources
    pub async fn release_resources(&self, request_id: &str) -> Result<(), String> {
        // Remove from active allocations
        let request = {
            let mut active = self.active_allocations.write().await;
            active
                .remove(request_id)
                .ok_or_else(|| format!("Request {} not found", request_id))?
        };

        // Release resources back to pools
        let mut pools = self.resource_pools.write().await;
        for (resource_type, amount) in &request.resources {
            if let Some(pool) = pools.get_mut(resource_type) {
                pool.release(*amount)?;
            }
        }

        Ok(())
    }

    /// Calculate allocation cost
    async fn calculate_cost(
        &self,
        allocated: &HashMap<ResourceType, f64>,
        request: &ResourceRequest,
    ) -> Result<f64, String> {
        let cost_settings = self.cost_settings.read().await;

        let duration_hours = request.duration_estimate_ms as f64 / (1000.0 * 60.0 * 60.0);

        let mut total_cost = 0.0;

        for (resource_type, amount) in allocated {
            let resource_cost = match resource_type {
                ResourceType::Cpu => amount * cost_settings.hourly_cost_cpu * duration_hours,
                ResourceType::Memory => {
                    (amount / 1024.0) * cost_settings.hourly_cost_memory_gb * duration_hours
                }
                ResourceType::Gpu => amount * cost_settings.hourly_cost_gpu * duration_hours,
                ResourceType::Network => {
                    (amount / 1024.0) * cost_settings.hourly_cost_network_gb * duration_hours
                }
                ResourceType::Storage => {
                    (amount / 1024.0) * cost_settings.hourly_cost_storage_gb * duration_hours
                }
                ResourceType::ApiQuota => {
                    // API quota is typically priced per call
                    amount * 0.001
                }
            };
            total_cost += resource_cost;
        }

        Ok(total_cost)
    }

    /// Calculate allocation confidence
    async fn calculate_allocation_confidence(
        &self,
        request: &ResourceRequest,
        allocated: &HashMap<ResourceType, f64>,
    ) -> f64 {
        let mut confidence = 0.8;

        // Boost for high priority
        confidence += (request.priority as i32 as f64) * 0.02;

        // Reduce for large allocations
        let total_allocated: f64 = allocated.values().sum();
        if total_allocated > 100.0 {
            confidence -= 0.1;
        }

        // Reduce for long durations
        if request.duration_estimate_ms > 60000 {
            confidence -= 0.1;
        }

        confidence.clamp(0.0, 1.0)
    }

    /// Generate alternative suggestions
    async fn generate_alternatives(
        &self,
        request: &ResourceRequest,
        pools: &HashMap<ResourceType, ResourcePool>,
    ) -> Vec<String> {
        let mut suggestions = Vec::new();

        // Suggest reducing resource request
        for (resource_type, amount) in &request.resources {
            if let Some(pool) = pools.get(resource_type) {
                if pool.available < *amount {
                    suggestions.push(format!(
                        "Reduce {:?} request from {:.2} to {:.2}",
                        resource_type, amount, pool.available
                    ));
                }
            }
        }

        // Suggest waiting for resources
        suggestions.push("Wait for resources to become available".to_string());

        // Suggest priority increase
        if request.priority < AllocationPriority::High {
            suggestions.push("Increase allocation priority".to_string());
        }

        // Suggest alternative agents
        suggestions.push("Consider using alternative agent roles".to_string());

        suggestions
    }

    /// Get current utilization metrics
    pub async fn utilization_metrics(&self) -> UtilizationMetrics {
        let pools = self.resource_pools.read().await;
        let active = self.active_allocations.read().await;

        let cpu_util = pools
            .get(&ResourceType::Cpu)
            .map_or(0.0, |p| p.utilization_percent);

        let memory_util = pools
            .get(&ResourceType::Memory)
            .map_or(0.0, |p| p.utilization_percent);

        let gpu_util = pools
            .get(&ResourceType::Gpu)
            .map_or(0.0, |p| p.utilization_percent);

        let network_util = pools
            .get(&ResourceType::Network)
            .map_or(0.0, |p| p.allocated);

        let total_cost = self.calculate_total_cost(&pools).await;

        UtilizationMetrics {
            timestamp: Utc::now(),
            cpu_utilization_percent: cpu_util,
            memory_utilization_percent: memory_util,
            gpu_utilization_percent: gpu_util,
            network_utilization_mb: network_util,
            active_allocations: active.len(),
            pending_requests: 0, // Could track rejected requests
            total_cost_hourly: total_cost,
        }
    }

    /// Calculate total cost per hour
    async fn calculate_total_cost(&self, pools: &HashMap<ResourceType, ResourcePool>) -> f64 {
        let cost_settings = self.cost_settings.read().await;

        let mut total_cost = 0.0;

        if let Some(cpu_pool) = pools.get(&ResourceType::Cpu) {
            total_cost += cpu_pool.allocated * cost_settings.hourly_cost_cpu;
        }

        if let Some(memory_pool) = pools.get(&ResourceType::Memory) {
            total_cost += (memory_pool.allocated / 1024.0) * cost_settings.hourly_cost_memory_gb;
        }

        if let Some(gpu_pool) = pools.get(&ResourceType::Gpu) {
            total_cost += gpu_pool.allocated * cost_settings.hourly_cost_gpu;
        }

        total_cost
    }

    /// Forecast capacity needs
    pub async fn forecast_capacity(&self) -> Result<Vec<CapacityForecast>, String> {
        if !self.config.enable_capacity_planning {
            return Ok(Vec::new());
        }

        let metrics = self.utilization_metrics.read().await;
        let _pools = self.resource_pools.read().await;

        if metrics.is_empty() {
            return Ok(Vec::new());
        }

        let mut forecasts = Vec::new();
        let current_time = Utc::now();

        // Simple linear forecast based on historical trends
        let cpu_trend = self.calculate_trend(
            metrics
                .iter()
                .map(|m| m.cpu_utilization_percent)
                .collect::<Vec<_>>(),
        );

        let memory_trend = self.calculate_trend(
            metrics
                .iter()
                .map(|m| m.memory_utilization_percent)
                .collect::<Vec<_>>(),
        );

        // Generate forecast for next 24 hours
        for hours_ahead in 1..=self.config.forecasting_horizon_hours {
            let forecast_time = current_time + chrono::Duration::hours(hours_ahead as i64);

            let predicted_cpu = cpu_trend.mul_add(
                hours_ahead as f64,
                metrics.last().map_or(0.0, |m| m.cpu_utilization_percent),
            );

            let predicted_memory = memory_trend.mul_add(
                hours_ahead as f64,
                metrics.last().map_or(0.0, |m| m.memory_utilization_percent),
            );

            let cost_settings = self.cost_settings.read().await;
            let scale_action = if predicted_cpu > cost_settings.target_utilization_percent + 20.0
                || predicted_memory > cost_settings.target_utilization_percent + 20.0
            {
                // Scale up recommended
                let mut resources = HashMap::new();
                resources.insert(ResourceType::Cpu, 20.0);
                resources.insert(ResourceType::Memory, 4096.0);
                ScaleAction::ScaleUp { resources }
            } else if predicted_cpu < cost_settings.target_utilization_percent - 20.0
                && predicted_memory < cost_settings.target_utilization_percent - 20.0
            {
                // Scale down recommended
                let mut resources = HashMap::new();
                resources.insert(ResourceType::Cpu, -10.0);
                resources.insert(ResourceType::Memory, -2048.0);
                ScaleAction::ScaleDown { resources }
            } else {
                ScaleAction::NoAction
            };

            forecasts.push(CapacityForecast {
                forecast_date: forecast_time,
                predicted_cpu_utilization: predicted_cpu.clamp(0.0, 100.0),
                predicted_memory_utilization: predicted_memory.clamp(0.0, 100.0),
                recommended_scale_action: scale_action,
                confidence: 0.7, // Simple linear forecast has moderate confidence
            });
        }

        Ok(forecasts)
    }

    /// Calculate trend from historical data
    fn calculate_trend(&self, data: Vec<f64>) -> f64 {
        if data.len() < 2 {
            return 0.0;
        }

        // Simple linear regression slope
        let n = data.len() as f64;
        let sum_x: f64 = (0..data.len()).map(|i| i as f64).sum();
        let sum_y: f64 = data.iter().sum();
        let sum_xy: f64 = data.iter().enumerate().map(|(i, y)| i as f64 * y).sum();
        let sum_x2: f64 = (0..data.len()).map(|i| (i as f64).powi(2)).sum();

        n.mul_add(sum_xy, -(sum_x * sum_y)) / sum_x.mul_add(-sum_x, n * sum_x2)
    }

    /// Record utilization metrics
    pub async fn record_metrics(&self, metrics: UtilizationMetrics) {
        let mut history = self.utilization_metrics.write().await;
        history.push(metrics);

        // Trim to configured size
        if history.len() > self.config.utilization_history_size {
            history.remove(0);
        }
    }

    /// Get resource pool status
    pub async fn pool_status(&self) -> HashMap<ResourceType, ResourcePool> {
        let pools = self.resource_pools.read().await;
        pools.clone()
    }

    /// Update cost settings
    pub async fn update_cost_settings(&self, settings: CostOptimization) {
        let mut cost_settings = self.cost_settings.write().await;
        *cost_settings = settings;
    }

    /// Get allocation statistics
    pub async fn allocation_stats(&self) -> AllocationStatistics {
        let active = self.active_allocations.read().await;
        let history = self.allocation_history.read().await;

        let total_requests = history.len();
        let approved_requests = history.iter().filter(|d| d.approved).count();
        let rejected_requests = total_requests - approved_requests;

        let total_cost = history
            .iter()
            .filter(|d| d.approved)
            .map(|d| d.estimated_cost)
            .sum::<f64>();

        let average_approval_time = if history.is_empty() {
            0.0
        } else {
            // Placeholder - would need actual timing data
            100.0
        };

        AllocationStatistics {
            total_requests,
            active_allocations: active.len(),
            approved_requests,
            rejected_requests,
            approval_rate: if total_requests > 0 {
                approved_requests as f64 / total_requests as f64
            } else {
                0.0
            },
            total_cost,
            average_cost_per_request: if approved_requests > 0 {
                total_cost / approved_requests as f64
            } else {
                0.0
            },
            average_approval_time_ms: average_approval_time,
        }
    }

    /// Scale resources based on forecasts
    pub async fn auto_scale(&self) -> Result<ScaleAction, String> {
        if !self.config.enable_auto_scaling {
            return Ok(ScaleAction::NoAction);
        }

        let forecasts = self.forecast_capacity().await?;

        if let Some(forecast) = forecasts.first() {
            match &forecast.recommended_scale_action {
                ScaleAction::ScaleUp { resources } => {
                    // Apply scale up
                    let mut pools = self.resource_pools.write().await;
                    for (resource_type, amount) in resources {
                        if let Some(pool) = pools.get_mut(resource_type) {
                            pool.total_capacity += amount;
                            pool.available += amount;
                            pool.calculate_utilization();
                        }
                    }
                    Ok(ScaleAction::ScaleUp {
                        resources: resources.clone(),
                    })
                }
                ScaleAction::ScaleDown { resources } => {
                    // Check if scale down is safe
                    let pools = self.resource_pools.read().await;
                    for (resource_type, amount) in resources {
                        if let Some(pool) = pools.get(resource_type) {
                            if pool.allocated + pool.reserved > pool.total_capacity + amount {
                                return Ok(ScaleAction::ScaleDownRestricted {
                                    reason: "Cannot scale down - resources are in use".to_string(),
                                });
                            }
                        }
                    }

                    // Apply scale down
                    drop(pools);
                    let mut pools = self.resource_pools.write().await;
                    for (resource_type, amount) in resources {
                        if let Some(pool) = pools.get_mut(resource_type) {
                            let reduction = amount.abs();
                            pool.total_capacity -= reduction;
                            pool.available = (pool.available - reduction).max(0.0);
                            pool.calculate_utilization();
                        }
                    }
                    Ok(ScaleAction::ScaleDown {
                        resources: resources.clone(),
                    })
                }
                _ => Ok(ScaleAction::NoAction),
            }
        } else {
            Ok(ScaleAction::NoAction)
        }
    }
}

/// Allocation statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocationStatistics {
    pub total_requests: usize,
    pub active_allocations: usize,
    pub approved_requests: usize,
    pub rejected_requests: usize,
    pub approval_rate: f64,
    pub total_cost: f64,
    pub average_cost_per_request: f64,
    pub average_approval_time_ms: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_resource_pool_utilization() {
        let mut pool = ResourcePool {
            resource_type: ResourceType::Cpu,
            total_capacity: 100.0,
            allocated: 0.0,
            reserved: 0.0,
            available: 100.0,
            utilization_percent: 0.0,
            last_updated: Utc::now(),
        };

        assert!(pool.has_capacity(50.0));
        assert_eq!(pool.reserve(50.0), Ok(()));
        assert_eq!(pool.available, 50.0);

        assert_eq!(pool.allocate(50.0), Ok(()));
        assert_eq!(pool.allocated, 50.0);
        assert_eq!(pool.utilization_percent, 50.0);

        assert_eq!(pool.release(25.0), Ok(()));
        assert_eq!(pool.allocated, 25.0);
    }

    #[test]
    fn test_insufficient_capacity() {
        let mut pool = ResourcePool {
            resource_type: ResourceType::Cpu,
            total_capacity: 100.0,
            allocated: 0.0,
            reserved: 0.0,
            available: 100.0,
            utilization_percent: 0.0,
            last_updated: Utc::now(),
        };

        assert_eq!(
            pool.reserve(150.0),
            Err("Insufficient capacity: requested 150, available 100".to_string())
        );
    }

    #[tokio::test]
    async fn test_resource_request() {
        let mut initial_pools = HashMap::new();
        initial_pools.insert(ResourceType::Cpu, 100.0);
        initial_pools.insert(ResourceType::Memory, 16384.0);

        let manager = ResourceManager::new(initial_pools, ResourceManagerConfig::default());

        let mut resources = HashMap::new();
        resources.insert(ResourceType::Cpu, 25.0);
        resources.insert(ResourceType::Memory, 4096.0);

        let request = ResourceRequest {
            request_id: String::new(),
            agent_role: AgentRole::Reviewer,
            task_id: "test_task".to_string(),
            resources,
            priority: AllocationPriority::High,
            max_cost: 10.0,
            duration_estimate_ms: 3600000, // 1 hour
            created_at: Utc::now(),
            deadline: None,
        };

        let decision = manager.request_resources(request).await.unwrap();
        assert!(decision.approved);
        assert_eq!(
            decision.allocated_resources.get(&ResourceType::Cpu),
            Some(&25.0)
        );
        assert_eq!(
            decision.allocated_resources.get(&ResourceType::Memory),
            Some(&4096.0)
        );
    }

    #[tokio::test]
    async fn test_budget_constraint() {
        let mut initial_pools = HashMap::new();
        initial_pools.insert(ResourceType::Gpu, 10.0);

        let manager = ResourceManager::new(initial_pools, ResourceManagerConfig::default());

        let mut resources = HashMap::new();
        resources.insert(ResourceType::Gpu, 10.0);

        let request = ResourceRequest {
            request_id: String::new(),
            agent_role: AgentRole::Researcher,
            task_id: "test_task".to_string(),
            resources,
            priority: AllocationPriority::High,
            max_cost: 0.5,                 // Very low budget
            duration_estimate_ms: 3600000, // 1 hour = $10 per GPU
            created_at: Utc::now(),
            deadline: None,
        };

        let decision = manager.request_resources(request).await.unwrap();
        assert!(!decision.approved);
        assert!(decision.reason.contains("Cost"));
    }

    #[tokio::test]
    async fn test_resource_release() {
        let mut initial_pools = HashMap::new();
        initial_pools.insert(ResourceType::Cpu, 100.0);

        let manager = ResourceManager::new(initial_pools, ResourceManagerConfig::default());

        let mut resources = HashMap::new();
        resources.insert(ResourceType::Cpu, 50.0);

        let request = ResourceRequest {
            request_id: "test_req".to_string(),
            agent_role: AgentRole::Reviewer,
            task_id: "test_task".to_string(),
            resources,
            priority: AllocationPriority::Medium,
            max_cost: 10.0,
            duration_estimate_ms: 1000,
            created_at: Utc::now(),
            deadline: None,
        };

        let decision = manager.request_resources(request).await.unwrap();
        assert!(decision.approved);

        // Verify resources are allocated
        let pools = manager.pool_status().await;
        let cpu_pool = pools.get(&ResourceType::Cpu).unwrap();
        assert_eq!(cpu_pool.allocated, 50.0);

        // Release resources
        manager.release_resources("test_req").await.unwrap();

        // Verify resources are released
        let pools = manager.pool_status().await;
        let cpu_pool = pools.get(&ResourceType::Cpu).unwrap();
        assert_eq!(cpu_pool.allocated, 0.0);
    }

    #[tokio::test]
    async fn test_utilization_metrics() {
        let mut initial_pools = HashMap::new();
        initial_pools.insert(ResourceType::Cpu, 100.0);
        initial_pools.insert(ResourceType::Memory, 16384.0);

        let manager = ResourceManager::new(initial_pools, ResourceManagerConfig::default());

        let metrics = manager.utilization_metrics().await;
        assert_eq!(metrics.cpu_utilization_percent, 0.0);
        assert_eq!(metrics.memory_utilization_percent, 0.0);
        assert_eq!(metrics.active_allocations, 0);
    }

    // --- New tests below ---

    #[test]
    fn test_resource_pool_release_more_than_allocated() {
        let mut pool = ResourcePool {
            resource_type: ResourceType::Cpu,
            total_capacity: 100.0,
            allocated: 30.0,
            reserved: 0.0,
            available: 70.0,
            utilization_percent: 30.0,
            last_updated: Utc::now(),
        };

        let result = pool.release(50.0);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Cannot release more than allocated"));
    }

    #[test]
    fn test_resource_pool_allocate_more_than_reserved() {
        let mut pool = ResourcePool {
            resource_type: ResourceType::Cpu,
            total_capacity: 100.0,
            allocated: 0.0,
            reserved: 20.0,
            available: 80.0,
            utilization_percent: 0.0,
            last_updated: Utc::now(),
        };

        let result = pool.allocate(30.0);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Cannot allocate more than reserved"));
    }

    #[test]
    fn test_resource_pool_zero_capacity() {
        let mut pool = ResourcePool {
            resource_type: ResourceType::Gpu,
            total_capacity: 0.0,
            allocated: 0.0,
            reserved: 0.0,
            available: 0.0,
            utilization_percent: 0.0,
            last_updated: Utc::now(),
        };

        pool.calculate_utilization();
        assert_eq!(pool.utilization_percent, 0.0);
        assert!(!pool.has_capacity(1.0));
    }

    #[test]
    fn test_cost_optimization_default() {
        let cost = CostOptimization::default();
        assert_eq!(cost.hourly_cost_cpu, 0.05);
        assert_eq!(cost.hourly_cost_memory_gb, 0.01);
        assert_eq!(cost.hourly_cost_gpu, 1.0);
        assert_eq!(cost.hourly_cost_network_gb, 0.01);
        assert_eq!(cost.hourly_cost_storage_gb, 0.001);
        assert_eq!(cost.budget_limit_hourly, 10.0);
        assert_eq!(cost.budget_limit_daily, 100.0);
        assert!(cost.enable_auto_scaling);
        assert_eq!(cost.target_utilization_percent, 70.0);
    }

    #[test]
    fn test_resource_manager_config_default() {
        let config = ResourceManagerConfig::default();
        assert!(config.enable_cost_optimization);
        assert!(config.enable_capacity_planning);
        assert!(config.enable_auto_scaling);
        assert_eq!(config.forecasting_horizon_hours, 24);
        assert_eq!(config.utilization_history_size, 1000);
        assert_eq!(config.allocation_timeout_ms, 5000);
    }

    #[tokio::test]
    async fn test_resource_request_missing_pool() {
        let initial_pools = HashMap::new(); // No pools at all
        let manager = ResourceManager::new(initial_pools, ResourceManagerConfig::default());

        let mut resources = HashMap::new();
        resources.insert(ResourceType::Cpu, 10.0);

        let request = ResourceRequest {
            request_id: String::new(),
            agent_role: AgentRole::Reviewer,
            task_id: "task1".to_string(),
            resources,
            priority: AllocationPriority::Medium,
            max_cost: 100.0,
            duration_estimate_ms: 1000,
            created_at: Utc::now(),
            deadline: None,
        };

        let decision = manager.request_resources(request).await.unwrap();
        assert!(!decision.approved);
        assert!(decision.reason.contains("resource pool not found"));
    }

    #[tokio::test]
    async fn test_release_nonexistent_request() {
        let mut initial_pools = HashMap::new();
        initial_pools.insert(ResourceType::Cpu, 100.0);

        let manager = ResourceManager::new(initial_pools, ResourceManagerConfig::default());

        let result = manager.release_resources("nonexistent").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_allocation_stats() {
        let mut initial_pools = HashMap::new();
        initial_pools.insert(ResourceType::Cpu, 100.0);

        let manager = ResourceManager::new(initial_pools, ResourceManagerConfig::default());

        // Initially empty
        let stats = manager.allocation_stats().await;
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.approval_rate, 0.0);

        // Make a request
        let mut resources = HashMap::new();
        resources.insert(ResourceType::Cpu, 10.0);

        let request = ResourceRequest {
            request_id: String::new(),
            agent_role: AgentRole::Reviewer,
            task_id: "t1".to_string(),
            resources,
            priority: AllocationPriority::High,
            max_cost: 100.0,
            duration_estimate_ms: 1000,
            created_at: Utc::now(),
            deadline: None,
        };

        manager.request_resources(request).await.unwrap();

        let stats = manager.allocation_stats().await;
        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.approved_requests, 1);
        assert_eq!(stats.rejected_requests, 0);
        assert_eq!(stats.active_allocations, 1);
        assert_eq!(stats.approval_rate, 1.0);
    }

    #[tokio::test]
    async fn test_update_cost_settings() {
        let mut initial_pools = HashMap::new();
        initial_pools.insert(ResourceType::Cpu, 100.0);

        let manager = ResourceManager::new(initial_pools, ResourceManagerConfig::default());

        let new_settings = CostOptimization {
            hourly_cost_cpu: 0.10,
            budget_limit_hourly: 5.0,
            ..Default::default()
        };

        manager.update_cost_settings(new_settings).await;

        // Verify the new settings take effect by making a request
        let mut resources = HashMap::new();
        resources.insert(ResourceType::Cpu, 10.0);

        let request = ResourceRequest {
            request_id: String::new(),
            agent_role: AgentRole::Reviewer,
            task_id: "t1".to_string(),
            resources,
            priority: AllocationPriority::High,
            max_cost: 100.0,
            duration_estimate_ms: 3600000,
            created_at: Utc::now(),
            deadline: None,
        };

        let decision = manager.request_resources(request).await.unwrap();
        assert!(decision.approved);
        // With $0.10/cpu * 10 * 1hr = $1.00
        assert!(decision.estimated_cost > 0.0);
    }

    #[tokio::test]
    async fn test_get_pool_status() {
        let mut initial_pools = HashMap::new();
        initial_pools.insert(ResourceType::Cpu, 100.0);
        initial_pools.insert(ResourceType::Memory, 8192.0);

        let manager = ResourceManager::new(initial_pools, ResourceManagerConfig::default());

        let status = manager.pool_status().await;
        assert_eq!(status.len(), 2);
        assert!(status.contains_key(&ResourceType::Cpu));
        assert!(status.contains_key(&ResourceType::Memory));

        let cpu_pool = status.get(&ResourceType::Cpu).unwrap();
        assert_eq!(cpu_pool.total_capacity, 100.0);
        assert_eq!(cpu_pool.allocated, 0.0);
    }

    #[tokio::test]
    async fn test_record_metrics_and_forecast_disabled() {
        let mut initial_pools = HashMap::new();
        initial_pools.insert(ResourceType::Cpu, 100.0);

        let config = ResourceManagerConfig {
            enable_capacity_planning: false,
            ..Default::default()
        };
        let manager = ResourceManager::new(initial_pools, config);

        let forecasts = manager.forecast_capacity().await.unwrap();
        assert!(forecasts.is_empty());
    }

    #[tokio::test]
    async fn test_record_metrics_and_forecast_empty() {
        let mut initial_pools = HashMap::new();
        initial_pools.insert(ResourceType::Cpu, 100.0);

        let manager = ResourceManager::new(initial_pools, ResourceManagerConfig::default());

        // No metrics recorded, forecast should be empty
        let forecasts = manager.forecast_capacity().await.unwrap();
        assert!(forecasts.is_empty());
    }

    #[tokio::test]
    async fn test_auto_scale_disabled() {
        let mut initial_pools = HashMap::new();
        initial_pools.insert(ResourceType::Cpu, 100.0);

        let config = ResourceManagerConfig {
            enable_auto_scaling: false,
            ..Default::default()
        };
        let manager = ResourceManager::new(initial_pools, config);

        let action = manager.auto_scale().await.unwrap();
        assert_eq!(action, ScaleAction::NoAction);
    }

    #[test]
    fn test_allocation_priority_ordering() {
        assert!(AllocationPriority::Critical > AllocationPriority::High);
        assert!(AllocationPriority::High > AllocationPriority::Medium);
        assert!(AllocationPriority::Medium > AllocationPriority::Low);
        assert!(AllocationPriority::Low > AllocationPriority::Background);
    }

    #[test]
    fn test_resource_type_serialization_roundtrip() {
        for rt in [
            ResourceType::Cpu,
            ResourceType::Memory,
            ResourceType::Gpu,
            ResourceType::Network,
            ResourceType::Storage,
            ResourceType::ApiQuota,
        ] {
            let json = serde_json::to_string(&rt).unwrap();
            let back: ResourceType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, rt);
        }
    }

    #[test]
    fn test_scale_action_equality() {
        let action = ScaleAction::NoAction;
        assert_eq!(action, ScaleAction::NoAction);

        let up = ScaleAction::ScaleUp {
            resources: HashMap::from([(ResourceType::Cpu, 10.0)]),
        };
        assert_eq!(
            up,
            ScaleAction::ScaleUp {
                resources: HashMap::from([(ResourceType::Cpu, 10.0)]),
            }
        );

        let restricted = ScaleAction::ScaleUpRestricted {
            reason: "budget".to_string(),
        };
        assert_eq!(
            restricted,
            ScaleAction::ScaleUpRestricted {
                reason: "budget".to_string(),
            }
        );
    }

    #[test]
    fn test_calculate_trend() {
        let mut initial_pools = HashMap::new();
        initial_pools.insert(ResourceType::Cpu, 100.0);

        let manager = ResourceManager::new(initial_pools, ResourceManagerConfig::default());

        // Increasing trend
        let trend = manager.calculate_trend(vec![10.0, 20.0, 30.0, 40.0, 50.0]);
        assert!(trend > 0.0);

        // Flat trend
        let trend = manager.calculate_trend(vec![50.0, 50.0, 50.0]);
        assert!(trend.abs() < 0.01);

        // Single data point
        let trend = manager.calculate_trend(vec![42.0]);
        assert_eq!(trend, 0.0);

        // Empty data
        let trend = manager.calculate_trend(vec![]);
        assert_eq!(trend, 0.0);
    }

    #[tokio::test]
    async fn test_resource_request_auto_id() {
        let mut initial_pools = HashMap::new();
        initial_pools.insert(ResourceType::Cpu, 100.0);

        let manager = ResourceManager::new(initial_pools, ResourceManagerConfig::default());

        let mut resources = HashMap::new();
        resources.insert(ResourceType::Cpu, 10.0);

        let request = ResourceRequest {
            request_id: String::new(), // Empty, should be auto-generated
            agent_role: AgentRole::Reviewer,
            task_id: "t1".to_string(),
            resources,
            priority: AllocationPriority::Medium,
            max_cost: 100.0,
            duration_estimate_ms: 1000,
            created_at: Utc::now(),
            deadline: None,
        };

        let decision = manager.request_resources(request).await.unwrap();
        assert!(decision.approved);
        assert!(decision.request_id.starts_with("req_"));
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for resource_manager
    // =========================================================================

    // 1. AllocationPriority serde roundtrip all variants
    #[test]
    fn allocation_priority_serde_roundtrip() {
        let priorities = [
            AllocationPriority::Critical,
            AllocationPriority::High,
            AllocationPriority::Medium,
            AllocationPriority::Low,
            AllocationPriority::Background,
        ];
        for p in &priorities {
            let json = serde_json::to_string(p).unwrap();
            let decoded: AllocationPriority = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, decoded);
        }
    }

    // 2. AllocationPriority ordering values
    #[test]
    fn allocation_priority_values() {
        assert_eq!(AllocationPriority::Critical as i32, 5);
        assert_eq!(AllocationPriority::High as i32, 4);
        assert_eq!(AllocationPriority::Medium as i32, 3);
        assert_eq!(AllocationPriority::Low as i32, 2);
        assert_eq!(AllocationPriority::Background as i32, 1);
    }

    // 3. ResourcePool serde roundtrip
    #[test]
    fn resource_pool_serde_roundtrip() {
        let pool = ResourcePool {
            resource_type: ResourceType::Gpu,
            total_capacity: 8.0,
            allocated: 3.0,
            reserved: 1.0,
            available: 4.0,
            utilization_percent: 37.5,
            last_updated: Utc::now(),
        };
        let json = serde_json::to_string(&pool).unwrap();
        let decoded: ResourcePool = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.resource_type, ResourceType::Gpu);
        assert!((decoded.total_capacity - 8.0).abs() < f64::EPSILON);
        assert!((decoded.allocated - 3.0).abs() < f64::EPSILON);
    }

    // 4. ResourceRequest serde roundtrip
    #[test]
    fn resource_request_serde_roundtrip() {
        let req = ResourceRequest {
            request_id: "req_42".into(),
            agent_role: AgentRole::Skeptic,
            task_id: "task_x".into(),
            resources: HashMap::from([(ResourceType::Cpu, 25.0), (ResourceType::Memory, 4096.0)]),
            priority: AllocationPriority::High,
            max_cost: 50.0,
            duration_estimate_ms: 60000,
            created_at: Utc::now(),
            deadline: Some(Utc::now()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: ResourceRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.request_id, "req_42");
        assert_eq!(decoded.agent_role, AgentRole::Skeptic);
        assert_eq!(decoded.resources.len(), 2);
        assert!(decoded.deadline.is_some());
    }

    // 5. AllocationDecision serde roundtrip
    #[test]
    fn allocation_decision_serde_roundtrip() {
        let decision = AllocationDecision {
            request_id: "req_1".into(),
            approved: true,
            allocated_resources: HashMap::from([(ResourceType::Cpu, 50.0)]),
            estimated_cost: 2.5,
            reason: "Sufficient capacity".into(),
            confidence: 0.95,
            alternative_suggestions: vec!["Use GPU instead".into()],
            allocated_at: Utc::now(),
        };
        let json = serde_json::to_string(&decision).unwrap();
        let decoded: AllocationDecision = serde_json::from_str(&json).unwrap();
        assert!(decoded.approved);
        assert_eq!(decoded.allocated_resources.len(), 1);
        assert_eq!(decoded.alternative_suggestions.len(), 1);
    }

    // 6. CostOptimization serde roundtrip
    #[test]
    fn cost_optimization_serde_roundtrip() {
        let cost = CostOptimization {
            hourly_cost_cpu: 0.08,
            hourly_cost_memory_gb: 0.02,
            hourly_cost_gpu: 2.0,
            hourly_cost_network_gb: 0.005,
            hourly_cost_storage_gb: 0.0005,
            budget_limit_hourly: 20.0,
            budget_limit_daily: 200.0,
            enable_auto_scaling: false,
            target_utilization_percent: 80.0,
        };
        let json = serde_json::to_string(&cost).unwrap();
        let decoded: CostOptimization = serde_json::from_str(&json).unwrap();
        assert!((decoded.hourly_cost_cpu - 0.08).abs() < f64::EPSILON);
        assert!(!decoded.enable_auto_scaling);
        assert!((decoded.target_utilization_percent - 80.0).abs() < f64::EPSILON);
    }

    // 7. CapacityForecast serde roundtrip
    #[test]
    fn capacity_forecast_serde_roundtrip() {
        let forecast = CapacityForecast {
            forecast_date: Utc::now(),
            predicted_cpu_utilization: 65.0,
            predicted_memory_utilization: 40.0,
            recommended_scale_action: ScaleAction::NoAction,
            confidence: 0.85,
        };
        let json = serde_json::to_string(&forecast).unwrap();
        let decoded: CapacityForecast = serde_json::from_str(&json).unwrap();
        assert!((decoded.predicted_cpu_utilization - 65.0).abs() < f64::EPSILON);
        assert_eq!(decoded.recommended_scale_action, ScaleAction::NoAction);
    }

    // 8. UtilizationMetrics serde roundtrip
    #[test]
    fn utilization_metrics_serde_roundtrip() {
        let metrics = UtilizationMetrics {
            timestamp: Utc::now(),
            cpu_utilization_percent: 72.5,
            memory_utilization_percent: 55.0,
            gpu_utilization_percent: 10.0,
            network_utilization_mb: 2048.0,
            active_allocations: 5,
            pending_requests: 2,
            total_cost_hourly: 3.75,
        };
        let json = serde_json::to_string(&metrics).unwrap();
        let decoded: UtilizationMetrics = serde_json::from_str(&json).unwrap();
        assert!((decoded.cpu_utilization_percent - 72.5).abs() < f64::EPSILON);
        assert_eq!(decoded.active_allocations, 5);
        assert_eq!(decoded.pending_requests, 2);
    }

    // 9. ResourceManagerConfig serde roundtrip
    #[test]
    fn resource_manager_config_serde_roundtrip() {
        let config = ResourceManagerConfig {
            enable_cost_optimization: false,
            enable_capacity_planning: true,
            enable_auto_scaling: true,
            forecasting_horizon_hours: 48,
            utilization_history_size: 500,
            allocation_timeout_ms: 10000,
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: ResourceManagerConfig = serde_json::from_str(&json).unwrap();
        assert!(!decoded.enable_cost_optimization);
        assert_eq!(decoded.forecasting_horizon_hours, 48);
        assert_eq!(decoded.allocation_timeout_ms, 10000);
    }

    // 10. ScaleAction ScaleDown serde roundtrip
    #[test]
    fn scale_action_down_serde_roundtrip() {
        let action = ScaleAction::ScaleDown {
            resources: HashMap::from([(ResourceType::Gpu, -2.0)]),
        };
        let json = serde_json::to_string(&action).unwrap();
        let decoded: ScaleAction = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, action);
    }

    // 11. ScaleAction ScaleDownRestricted serde roundtrip
    #[test]
    fn scale_action_down_restricted_serde_roundtrip() {
        let action = ScaleAction::ScaleDownRestricted {
            reason: "Resources in use".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let decoded: ScaleAction = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, action);
    }

    // 12. AllocationStatistics serde roundtrip
    #[test]
    fn allocation_statistics_serde_roundtrip() {
        let stats = AllocationStatistics {
            total_requests: 100,
            active_allocations: 15,
            approved_requests: 85,
            rejected_requests: 15,
            approval_rate: 0.85,
            total_cost: 42.5,
            average_cost_per_request: 0.5,
            average_approval_time_ms: 120.0,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: AllocationStatistics = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_requests, 100);
        assert_eq!(decoded.active_allocations, 15);
        assert!((decoded.approval_rate - 0.85).abs() < f64::EPSILON);
    }

    // 13. ResourcePool debug format
    #[test]
    fn resource_pool_debug_format() {
        let pool = ResourcePool {
            resource_type: ResourceType::Cpu,
            total_capacity: 100.0,
            allocated: 50.0,
            reserved: 10.0,
            available: 40.0,
            utilization_percent: 50.0,
            last_updated: Utc::now(),
        };
        let debug = format!("{:?}", pool);
        assert!(debug.contains("Cpu"));
        assert!(debug.contains("total_capacity"));
    }

    // 14. ResourceRequest with no deadline serde roundtrip
    #[test]
    fn resource_request_no_deadline_serde() {
        let req = ResourceRequest {
            request_id: "req_nd".into(),
            agent_role: AgentRole::Reviewer,
            task_id: "task_nd".into(),
            resources: HashMap::new(),
            priority: AllocationPriority::Low,
            max_cost: 0.0,
            duration_estimate_ms: 0,
            created_at: Utc::now(),
            deadline: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: ResourceRequest = serde_json::from_str(&json).unwrap();
        assert!(decoded.deadline.is_none());
        assert!(decoded.resources.is_empty());
    }

    // 15. AllocationDecision rejected serde roundtrip
    #[test]
    fn allocation_decision_rejected_serde() {
        let decision = AllocationDecision {
            request_id: "req_rej".into(),
            approved: false,
            allocated_resources: HashMap::new(),
            estimated_cost: 0.0,
            reason: "Insufficient budget".into(),
            confidence: 0.0,
            alternative_suggestions: vec![],
            allocated_at: Utc::now(),
        };
        let json = serde_json::to_string(&decision).unwrap();
        let decoded: AllocationDecision = serde_json::from_str(&json).unwrap();
        assert!(!decoded.approved);
        assert!(decoded.allocated_resources.is_empty());
        assert!(decoded.alternative_suggestions.is_empty());
    }
}
