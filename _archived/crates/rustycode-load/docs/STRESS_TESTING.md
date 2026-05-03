# Stress Testing Guide

Comprehensive guide to stress testing with rustycode-load for finding system breaking points.

## Overview

Stress testing goes beyond normal load testing by progressively increasing load until system failure is detected. This helps identify:

- **Breaking Points**: The maximum load a system can handle before failure
- **Failure Modes**: How the system fails (error rates, timeouts, resource exhaustion)
- **Recovery Characteristics**: How quickly and effectively the system recovers
- **Safety Margins**: Recommended production limits with safety buffers

## Quick Start

```rust
use rustycode_load::*;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create a stress test
    let stress_test = StressTest::builder("API Stress Test".to_string())
        .max_concurrent_users(1000)
        .initial_load(10)
        .load_increment(50)
        .step_duration(Duration::from_secs(30))
        .error_threshold(0.05) // 5% error rate
        .response_time_threshold(Duration::from_secs(5))
        .test_recovery(true)
        .request_generator(|user_id| {
            LoadRequest::http_get(format!("https://api.example.com/data/{}", user_id))
                .with_user_id(user_id)
        })
        .build()?;

    // Run the test
    let report = stress_test.run().await?;

    // Analyze results
    if let Some(breakpoint) = report.breaking_point {
        println!("Breaking point: {} users", breakpoint.load_level);
        println!("Failure type: {:?}", breakpoint.failure_type);
    }

    println!("Recommended production load: {}", report.summary.recommended_production_load);

    Ok(())
}
```

## Stress Test Configuration

### Core Parameters

- **max_concurrent_users**: Maximum users to test (default: 1000)
- **initial_load**: Starting load level (default: 10)
- **load_increment**: Users added per step (default: 50)
- **step_duration**: Time to maintain each load level (default: 30s)
- **max_duration**: Maximum total test duration (default: 600s)

### Failure Detection

- **error_threshold**: Error rate that triggers failure (default: 0.05 = 5%)
- **response_time_threshold**: P99 response time that triggers failure (default: 5s)

### Recovery Testing

- **test_recovery**: Whether to test recovery after failure (default: true)
- **recovery_duration**: Time to test recovery (default: 60s)
- **recovery_load_factor**: Load reduction for recovery test (default: 0.5 = 50%)

## Predefined Scenarios

### 1. Load Capacity Test

Standard progressive load test for finding capacity limits:

```rust
let test = StressTestScenarios::load_capacity_test("https://api.example.com".to_string());
let report = test.build()?.run().await?;
```

**Configuration:**
- Max users: 1000
- Initial load: 10
- Increment: 50
- Step duration: 30s
- Error threshold: 5%

### 2. Aggressive Stress Test

Rapid load increase for finding hard limits:

```rust
let test = StressTestScenarios::aggressive_stress_test("https://api.example.com".to_string());
let report = test.build()?.run().await?;
```

**Configuration:**
- Max users: 5000
- Initial load: 100
- Increment: 500
- Step duration: 15s
- Error threshold: 10%

### 3. Endurance Test

Long-running test for detecting resource leaks and degradation:

```rust
let test = StressTestScenarios::endurance_test("https://api.example.com".to_string());
let report = test.build()?.run().await?;
```

**Configuration:**
- Max users: 500
- Step duration: 5 minutes
- Max duration: 1 hour
- Error threshold: 1%

### 4. Spike Test

Sudden load increase for testing resilience:

```rust
let test = StressTestScenarios::spike_test("https://api.example.com".to_string());
let report = test.build()?.run().await?;
```

**Configuration:**
- Max users: 2000
- Initial load: 100
- Increment: 1000 (large jump)
- Recovery factor: 30%

## Understanding Results

### Breakpoint Analysis

A breakpoint contains:

```rust
pub struct Breakpoint {
    pub load_level: usize,           // Users at failure
    pub failure_type: FailureType,   // Type of failure
    pub error_rate: f64,             // Error rate at failure
    pub response_time: Duration,     // P99 response time
    pub description: String,         // Human-readable description
    pub recovery_result: Option<RecoveryResult>, // Recovery info
}
```

### Failure Types

- **ErrorRateExceeded**: Error rate surpassed threshold
- **ResponseTimeExceeded**: P99 response time too slow
- **SystemFailure**: Complete failure (100% errors)
- **ResourceExhaustion**: System resources depleted
- **ConnectionFailure**: Cannot establish connections
- **TimeoutStorm**: All requests timing out

### Recovery Analysis

If recovery testing is enabled:

```rust
pub struct RecoveryResult {
    pub successful: bool,            // Did recovery succeed?
    pub recovery_load: usize,        // Load during recovery
    pub time_to_recover: Duration,   // Time to stabilize
    pub error_rate_after: f64,       // Error rate after recovery
    pub stable: bool,                // Is system stable?
}
```

### Summary and Recommendations

The report includes:

```rust
pub struct StressTestSummary {
    pub max_sustainable_load: usize,      // Maximum stable load
    pub recommended_production_load: usize, // Safe production limit
    pub safety_margin: f64,               // Safety buffer percentage
    pub findings: Vec<String>,            // Key discoveries
    pub recommendations: Vec<String>,     // Action items
    pub critical_issues: Vec<String>,     // Critical problems
}
```

## Best Practices

### 1. Start Conservative

Begin with low initial loads and small increments:

```rust
.initial_load(10)
.load_increment(25)
.step_duration(Duration::from_secs(60))
```

### 2. Set Realistic Thresholds

Base thresholds on your SLA requirements:

```rust
.error_threshold(0.01) // 1% error rate
.response_time_threshold(Duration::from_millis(500)) // 500ms P99
```

### 3. Test Recovery

Always enable recovery testing to understand system resilience:

```rust
.test_recovery(true)
.recovery_load_factor(0.5) // Reduce to 50% load
```

### 4. Monitor Progress

Track test execution in real-time:

```rust
let progress = stress_test.progress().await;
println!("Current load: {}", progress.current_load);
println!("Breakpoints found: {}", progress.breakpoints_found);
```

### 5. Analyze Trends

Look at all step metrics, not just the breaking point:

```rust
for step in &report.step_metrics {
    println!("Load: {} | Errors: {:.2}% | P99: {:?}",
        step.load_level,
        step.error_rate * 100.0,
        step.p99
    );
}
```

## Advanced Usage

### Custom Request Generators

Create complex user behavior patterns:

```rust
.request_generator(move |user_id| {
    // Simulate realistic user behavior
    let action = match rand::random::<u8>() % 3 {
        0 => "read",
        1 => "write",
        _ => "search",
    };

    LoadRequest::http_get(format!("{}/api/{}", base_url, action))
        .with_user_id(user_id)
})
```

### Progressive Stress Testing

Run multiple tests with increasing intensity:

```rust
for max_users in [100, 500, 1000, 2000] {
    let test = StressTest::builder(format!("Test to {} users", max_users))
        .max_concurrent_users(max_users)
        // ... other config
        .build()?;

    let report = test.run().await?;
    // Analyze and compare results
}
```

### Baseline Comparison

Compare stress test results before and after changes:

```rust
// Run baseline test
let baseline = stress_test.run().await?;

// Make system changes

// Run comparison test
let comparison = stress_test.run().await?;

// Compare breaking points
if comparison.breaking_point.as_ref().map(|bp| bp.load_level)
    > baseline.breaking_point.as_ref().map(|bp| bp.load_level)
{
    println!("Performance improved!");
}
```

## Interpreting Results

### No Breaking Point Found

If the test completes without finding a breaking point:

1. Increase `max_concurrent_users`
2. Increase `load_increment` for faster testing
3. Consider if the test is realistic for your use case

### Early Breaking Point

If failure occurs at very low loads:

1. Check system logs for errors
2. Verify network connectivity
3. Review system resource usage
4. Ensure test configuration is correct

### Poor Recovery

If the system doesn't recover after load reduction:

1. **Critical Issue**: System may have resource leaks
2. Check for connection pool exhaustion
3. Review memory usage patterns
4. Examine database connection limits

### High Safety Margin

If the recommended load is much lower than breaking point:

1. Your system has good headroom
2. Consider if the breaking point test was aggressive enough
3. May indicate room for cost optimization

## Troubleshooting

### Test Too Slow

Increase load increment or reduce step duration:

```rust
.load_increment(100) // Larger jumps
.step_duration(Duration::from_secs(15)) // Shorter steps
```

### Too Many Timeouts

Increase request timeout or reduce load:

```rust
.request_generator(|user_id| {
    LoadRequest::http_get(url)
        .with_timeout(Duration::from_secs(60))
        .with_user_id(user_id)
})
```

### Inconsistent Results

1. Ensure no other load on the system
2. Run tests at consistent times
3. Use longer step durations for stability
4. Check for network fluctuations

## Integration with CI/CD

Add stress tests to your CI pipeline:

```yaml
# .github/workflows/stress-test.yml
name: Stress Tests
on: [push, pull_request]

jobs:
  stress:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Run stress tests
        run: |
          cargo run --release --bin stress_test
      - name: Check results
        run: |
          # Fail if breaking point is too low
          BREAKING_POINT=$(cat report.json | jq '.breaking_point.load_level')
          if [ $BREAKING_POINT -lt 100 ]; then
            echo "Breaking point too low: $BREAKING_POINT"
            exit 1
          fi
```

## Performance Considerations

### Test Machine Resources

Ensure the test machine has sufficient resources:
- CPU: At least 4 cores
- Memory: 8GB+ for high-concurrency tests
- Network: Low latency to target system

### Target System Impact

Stress tests will significantly impact the target system:
- Run during maintenance windows
- Monitor target system health
- Have rollback plans ready
- Never run against production without approval

### Resource Limits

Be aware of:
- File descriptor limits
- Network socket limits
- Memory constraints
- CPU throttling

## Conclusion

Stress testing is essential for understanding system limits and ensuring reliability. Use these tools to:

1. Find breaking points before your users do
2. Establish safe production limits
3. Verify system resilience
4. Plan capacity upgrades
5. Validate performance improvements

For more information, see the main rustycode-load documentation.
