# Panic Logging Examples

## Example 1: Basic Panic Scenario

### Handler Code
```rust
use axum::Json;

pub async fn process_transaction(
    Json(payload): Json<TransactionRequest>,
) -> Result<Json<TransactionResponse>, ApiError> {
    // Some processing...
    
    // Handler panics when encountering unexpected state
    let divisor = payload.amount.len() - payload.amount.len() - 1;  // Equals -1
    let result = 100 / divisor;  // PANIC: attempt to divide by zero
    
    Ok(Json(TransactionResponse { result }))
}
```

### What Happens

1. **Handler panics** with message: `"attempt to divide by zero"`

2. **Middleware catches panic**:
   - Detects `is_panic() == true` from task join error
   - Extracts panic message via `panic_value().downcast_ref()`

3. **Logs to structured log**:
```json
{
  "timestamp": "2024-11-21T10:15:30.123456Z",
  "level": "ERROR",
  "message": "Handler panic caught: attempt to divide by zero",
  "panic_message": "attempt to divide by zero",
  "backtrace": "#0  ... core::panic at library/core/src/panicking.rs:133\n#1  ...",
  "target": "sky_moon_scope_core::error_middleware",
  "span": { "name": "request" }
}
```

4. **Emits SysAlarm**:
```json
{
  "event": "handler_panic",
  "resource": "request_handler",
  "value_percent": 0.0,
  "threshold_percent": 0.0,
  "process_memory_bytes": 524288000,
  "total_memory_bytes": 8589934592,
  "pid": 12345,
  "node_id": "api-service-prod-01",
  "timestamp": "2024-11-21T10:15:30Z"
}
```

5. **POSTs to webhook** (if configured):
```
POST https://alerts.example.com/hook
Content-Type: application/json

{
  "event": "handler_panic",
  "resource": "request_handler",
  ...
}
```

6. **Returns to client**:
```
HTTP 500 Internal Server Error

Internal server error
```

---

## Example 2: Recurring Panic (Before vs After)

### Before Panic Logging

**Symptom**: Intermittent 500 errors in production

**Investigation without logging**:
- Check application logs: Empty (panic not logged)
- Check request traces: Shows HTTP 500 but no context
- Attach debugger: Can't reproduce locally
- Set up manual logging in handler: Too late, already deployed

**Result**: Hours of debugging, no clear root cause

### After Panic Logging

**Same symptom**: Intermittent 500 errors in production

**Investigation with panic logging**:

1. **Query logs immediately**:
```
filter level == ERROR AND panic_message != null
| stats count by panic_message
```

Results:
```
panic_message: "thread 'tokio-runtime-worker' panicked at 'called `Result::unwrap()`...'" - 45 occurrences
panic_message: "attempt to add with overflow" - 3 occurrences  
panic_message: "index out of bounds" - 12 occurrences
```

2. **Review backtrace for most frequent panic**:
```
#0 0x0000555555555555 in sky_moon_scope_core::handlers::process_data at core/src/handlers.rs:127
#1 0x0000555555555556 in tokio::runtime::task::harness::Harness::poll at /path/to/tokio/...
```

3. **Identify root cause**: Line 127 in `handlers.rs` unwraps a Result

4. **Fix and redeploy**: Add proper error handling

**Result**: 10 minute root cause analysis vs hours of debugging

---

## Example 3: Multi-Node Panic Detection

### Setup
- 3 service instances: `api-prod-01`, `api-prod-02`, `api-prod-03`
- Load balancer routing requests
- Panic occurs intermittently

### Log Query
```
filter event == "handler_panic"
| stats count by node_id, panic_message
```

### Results
```
node_id           | panic_message                    | count
api-prod-01       | "thread panicked at 'overflow'" | 47
api-prod-02       | (no panics)                     | 0
api-prod-03       | "thread panicked at 'overflow'" | 52
```

### Analysis
- Panic only occurs on 2 of 3 nodes
- Suggests environment-specific issue (memory state, CPU load, etc.)
- One node might have different data, config, or resource constraints

### Investigation Path
1. Compare configurations on api-prod-01 vs api-prod-02
2. Check data volume differences between nodes
3. Look at resource utilization when panic occurs
4. Check for time-based state differences (cached data, etc.)

---

## Example 4: Panic with Request Context

### Enhanced Logging (optional manual enrichment)

```rust
pub async fn process_transaction(
    Path(user_id): Path<i64>,
    Json(payload): Json<TransactionRequest>,
) -> Result<Json<TransactionResponse>, ApiError> {
    info!("Processing transaction", %user_id, amount = %payload.amount);
    
    // If panic occurs, manual log has context
    dangerous_operation(&payload).await?;
    
    Ok(Json(TransactionResponse { success: true }))
}
```

When panic occurs:
```json
{
  "timestamp": "2024-11-21T10:15:30Z",
  "level": "ERROR",
  "message": "Handler panic caught",
  "panic_message": "index out of bounds",
  "backtrace": "[...]",
  "related_spans": [
    {
      "name": "process_transaction",
      "user_id": "12345",
      "amount": "1000.00"
    }
  ]
}
```

---

## Example 5: Alarming Dashboard Setup

### Webhook Receiver Implementation

```rust
#[derive(Deserialize)]
struct AlarmEvent {
    event: String,
    resource: String,
    pid: u32,
    node_id: Option<String>,
    timestamp: chrono::DateTime<chrono::Utc>,
}

#[post("/alarms")]
async fn receive_alarm(Json(alarm): Json<AlarmEvent>) -> impl IntoResponse {
    match alarm.event.as_str() {
        "handler_panic" => {
            // Send to on-call Slack channel
            slack_notify(
                format!("🚨 Handler panic on {}",
                    alarm.node_id.unwrap_or("unknown".to_string()))
            ).await?;
            
            // Create incident in PagerDuty
            pagerduty_incident(
                "Handler panic",
                &format!("Node: {}, PID: {}", 
                    alarm.node_id.unwrap_or("?".to_string()),
                    alarm.pid)
            ).await?;
        }
        "threshold_breach" => {
            // Handle resource alarms separately
        }
        _ => {}
    }
    
    StatusCode::OK
}
```

### Grafana Dashboard Query

```promql
# Panic frequency (5-minute rate)
increase(handler_panics_total[5m])

# Panic frequency by node
increase(handler_panics_total{node_id!=""}[5m]) by (node_id)

# Cumulative panic count since deployment
handler_panics_total
```

---

## Example 6: Filtering Specific Panic Types

### Query: Find only memory-related panics

```
filter level == ERROR AND panic_message in [
  "*allocation failed*",
  "*memory allocation failed*", 
  "*out of memory*"
]
```

### Query: Find only logic panics

```
filter level == ERROR AND panic_message in [
  "*unwrap()*",
  "*expect()*",
  "*index out of bounds*",
  "*overflow*"
]
```

### Query: Find panics in specific handler

```
filter level == ERROR AND panic_message != null 
  AND backtrace contains "handlers.rs:127"
```

---

## Example 7: Testing Panic Logging Locally

### Create Test Handler

```rust
#[get("/test-panic")]
pub async fn test_panic() -> impl IntoResponse {
    panic!("This is a test panic for logging verification")
}
```

### Enable Backtrace

```bash
export RUST_BACKTRACE=1
export RUST_LOG=debug
```

### Run Server

```bash
cargo run --release
```

### Trigger Panic

```bash
curl http://localhost:3000/test-panic
```

### Observe Output

**Structured Log Output**:
```
ERROR sky_moon_scope_core::error_middleware: Handler panic caught: This is a test panic for logging verification
panic_message="This is a test panic for logging verification"
backtrace="#0  0x0000555555555555 in panicking ...
```

**Response**:
```
HTTP/1.1 500 Internal Server Error

Internal server error
```

**Webhook POST** (if configured):
```bash
# Monitor webhook server to see POST
# Or check webhook service logs
```

---

## Example 8: Correlating Panics with Metrics

### Scenario
- Panic spikes coincide with high CPU usage
- Need to correlate panic timing with system state

### Log Query
```
filter event == "handler_panic"
| extract timestamp as panic_time
| join (
    select timestamp, cpu_percent from metrics
    where timestamp between (panic_time - 5min) and (panic_time + 5min)
  ) using timestamp
| stats avg(cpu_percent) by panic_message
```

### Result
```
panic_message              | avg_cpu_percent
"stack overflow"           | 94.2
"allocation failed"        | 88.7
"thread panicked at 'div'" | 22.1
```

### Interpretation
- High-CPU panics: Likely resource exhaustion
- Low-CPU panics: Likely logic errors in code

---

## Example 9: Post-Incident Review

### Panic Incident Timeline

```
2024-11-21 10:15:30 - Panic begins (filtered logs)
error: Handler panic caught: "index out of bounds"
      panic_message="index out of bounds"
      backtrace="... handlers.rs:127 ..."

2024-11-21 10:15:30 - SysAlarm triggered (filtered alarms)
event="handler_panic"
node_id="api-prod-01"

2024-11-21 10:15:31 - Alert delivered to webhook
POST /alert - status 200

2024-11-21 10:15:35 - On-call notified (incident tracking)
Incident created: "Handler panic on api-prod-01"

2024-11-21 10:16:20 - Engineer investigates
Reviews backtrace, identifies bounds check missing

2024-11-21 10:18:45 - Fix deployed
Bounds check added to handlers.rs:127

2024-11-21 10:19:00 - Verification
No new panics in past 30 seconds
```

### Post-Incident Questions (Now Answerable)

1. **When did the panic start?** - 10:15:30 (from ERROR timestamp)
2. **What was the panic message?** - "index out of bounds"
3. **Which nodes were affected?** - api-prod-01
4. **How many times did it panic?** - count(*) where event="handler_panic"
5. **What was the root cause?** - Backtrace shows exact line: handlers.rs:127
6. **How quickly was it resolved?** - 3m 30s from panic to fix deployed

---

## Example 10: Alert Fatigue Prevention

### Without Rate Limiting

A flapping handler (e.g., upstream service timeout) causes panic every 100ms:

```
10:15:30 - Panic #1 - ALERT
10:15:30.1 - Panic #2 - ALERT
10:15:30.2 - Panic #3 - ALERT
... (1000+ alerts per second)
```

### With Rate Limiting

SysAlarm deduplication window (default 60 seconds):

```
10:15:30 - Panic #1 - ALERT SENT
10:15:30.1 - Panic #2 - SUPPRESSED (within 60s window)
10:15:30.2 - Panic #3 - SUPPRESSED
...
10:16:30 - Panic resumed - NEW ALERT SENT
```

### Configuration

```bash
# Custom dedup window (5 seconds)
export ALARM_DEDUP_WINDOW_SECS=5

# Disable rate limiting (alert on every panic)
export ALARM_DEDUP_WINDOW_SECS=0
```

---

## Troubleshooting Scenarios

### Scenario 1: Backtrace Shows `???`

**Cause**: Debug symbols not included in binary

**Solution**:
```bash
# Rebuild with debug symbols
cargo build --release

# Or set environment
export RUSTFLAGS="-g"
cargo build --release
```

### Scenario 2: Panic Message is Generic

**Example**: `"called Result::unwrap() on an Err value"`

**Root Cause**: Code is using `.unwrap()` instead of `?` or proper error handling

**Solution**:
- Check backtrace to find exact line
- Replace `.unwrap()` with `?` or `match`
- This also improves error handling

### Scenario 3: Webhook Not Receiving Alerts

**Causes**:
1. `Sky Moon Scope_ALARM_WEBHOOK_URL` not set
2. Webhook URL unreachable
3. Webhook server not listening

**Debug**:
```bash
# Check if env var is set
echo $Sky Moon Scope_ALARM_WEBHOOK_URL

# Test webhook connectivity
curl -X POST https://your-webhook-url -d '{"test": "data"}'

# Check webhook server is running
curl http://webhook-server:port/health
```

### Scenario 4: High Latency in Panic Path

**Cause**: Backtrace capture or webhook POST is slow

**Mitigation**:
- Backtrace capture is only on panic (rare)
- Webhook POST has 5-second timeout
- SysAlarm response doesn't block HTTP response

**Monitoring**:
```promql
histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[5m]))
```
