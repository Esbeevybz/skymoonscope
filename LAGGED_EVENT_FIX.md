# WebSocket Lagged Event Notification (Issue #565)

## Problem

When a WebSocket subscriber falls behind and the broadcast channel drops messages (lagged receiver), the subscription silently loses events without notifying the client. Clients that rely on completed events to unblock workflows can wait indefinitely without any indication that events were dropped.

## Solution

This fix detects `RecvError::Lagged` in both the WebSocket handler and gRPC service, and sends an explicit notification to the client so they can detect gaps and retry.

## Changes Made

### 1. **ws.rs** — WebSocket Event System

#### Added `LaggedPayload` struct
```rust
/// Emitted when the subscriber falls behind and the broadcast channel drops
/// messages. The client can use this to detect gaps and retry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaggedPayload {
    /// Number of events dropped due to the subscriber lagging.
    pub dropped: u64,
}
```

#### Extended `SimulationEvent` enum
Added a new `Lagged` variant to `SimulationEvent`:
```rust
Lagged {
    job_id: String,
    data: LaggedPayload,
    timestamp: DateTime<Utc>,
}
```

#### Updated `SimulationEvent` methods
- `job_id()` — Added pattern match for `Lagged` variant
- `is_terminal()` — Lagged events are non-terminal (connection continues)

#### Enhanced WebSocket handler (`handle_socket`)
When `RecvError::Lagged` is encountered:
1. Logs a warning about dropped events
2. Creates a `Lagged` event with the drop count
3. Serializes and sends it to the client as JSON
4. Continues listening for more events

**Before:**
```rust
Err(broadcast::error::RecvError::Lagged(n)) => {
    tracing::warn!(
        job_id = %job_id,
        skipped = n,
        "WebSocket consumer lagged — events were skipped"
    );
    // Keep going; the client just missed some progress ticks
}
```

**After:**
```rust
Err(broadcast::error::RecvError::Lagged(n)) => {
    tracing::warn!(
        job_id = %job_id,
        skipped = n,
        "WebSocket consumer lagged — events were skipped"
    );
    
    // Send explicit lagged notification to the client
    let lagged_event = SimulationEvent::Lagged {
        job_id: job_id.clone(),
        data: LaggedPayload { dropped: n },
        timestamp: Utc::now(),
    };
    
    let json = match serde_json::to_string(&lagged_event) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(
                job_id = %job_id,
                error = %e,
                "Failed to serialise Lagged event"
            );
            continue;
        }
    };
    
    if socket.send(Message::Text(json)).await.is_err() {
        // Client disconnected
        break;
    }
}
```

#### Added unit tests
- `lagged_event_serialises()` — Verifies JSON serialization
- `lagged_event_not_terminal()` — Confirms lagged events don't close the connection

### 2. **grpc.rs** — gRPC Service

#### Added `translate_lagged_event` helper function
Creates a Protobuf `ContractEvent` message with reserved event type `"lagged"`:
```rust
/// Create a lagged notification event for gRPC clients.
///
/// When the broadcast channel drops messages due to a slow subscriber,
/// we synthesize a special `ContractEvent` with a reserved event type
/// to notify the client about the dropped message count.
fn translate_lagged_event(dropped: u64) -> Option<ContractEvent> {
    Some(ContractEvent {
        ledger_sequence: 0,
        ledger_close_time: chrono::Utc::now().timestamp(),
        contract_id: String::new(), // no contract associated with this meta-event
        event_type: "lagged".to_string(),
        topics_json: "[]".to_string(),
        value_json: serde_json::json!({ "dropped": dropped }).to_string(),
        tx_hash: String::new(),
    })
}
```

#### Enhanced gRPC background task
When `RecvError::Lagged` is encountered:
1. Logs a warning about dropped events
2. Calls `translate_lagged_event` to create the notification
3. Sends it through the per-connection queue
4. Breaks the receive loop so the stream can close gracefully

**Before:**
```rust
Err(broadcast::error::RecvError::Lagged(skipped)) => {
    tracing::warn!(
        skipped = skipped,
        "gRPC stream lagged; some events were dropped"
    );
}
```

**After:**
```rust
Err(broadcast::error::RecvError::Lagged(skipped)) => {
    tracing::warn!(
        skipped = skipped,
        "gRPC stream lagged; some events were dropped"
    );
    // Send explicit lagged notification to the client before closing
    if let Some(lagged_event) = translate_lagged_event(skipped) {
        if let Err(e) = tx.try_send(lagged_event) {
            tracing::error!(
                error = ?e,
                "failed to send lagged event to gRPC stream"
            );
        }
    }
    // After notifying about the lag, stop consuming from the bus
    // so the stream can close gracefully
    break;
}
```

## Client Protocol

### WebSocket Client
Clients should monitor for the `"lagged"` event:
```json
{
  "event": "lagged",
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "data": { "dropped": 42 },
  "timestamp": "2026-04-25T12:00:00Z"
}
```

When received, the client can:
- Acknowledge the gap with `dropped` events missed
- Decide whether to continue from the current position or reconnect
- Implement exponential backoff if reconnecting

### gRPC Client
Clients should monitor for `ContractEvent` with `event_type == "lagged"`:
```protobuf
{
  "event_type": "lagged",
  "contract_id": "",  // empty for meta-events
  "value_json": "{\"dropped\": 42}"
}
```

When received, the client can:
- Parse the `dropped` count from `value_json`
- Understand that a gap occurred in the event stream
- Decide to reconnect or handle the gap gracefully

## Behavior Notes

1. **Non-terminal**: Lagged events do not close the WebSocket connection. The stream continues accepting more events after notifying the client.

2. **gRPC stream closure**: After sending a lagged event, the gRPC stream task breaks its receive loop and closes gracefully. This prevents the connection from stalling and allows the client to reconnect if desired.

3. **Error handling**: If serialization or sending the lagged event fails, the handler logs an error but doesn't crash the entire stream.

4. **Logging**: All lagged events are logged at `WARN` level for observability and debugging.

## Testing

Unit tests verify:
- JSON serialization of lagged events
- Lagged events are correctly identified as non-terminal
- Lagged events route to the correct job_id filter
- Round-trip serialization/deserialization works

## Future Enhancements

1. Add metrics/counters for lagged events
2. Implement exponential backoff retry logic on the client side
3. Consider adding a config option for maximum acceptable lag before forceful disconnection
4. Add integration tests with slow consumers to verify lagged behavior
