# Lagged Event Fix - Test Scenarios

This document describes how to manually test the lagged event functionality.

## Prerequisites

- Rust toolchain installed
- Ability to run the server (`cargo run -p sky-moon-scope-core`)
- A WebSocket client (e.g., websocat, or custom client)
- A gRPC client (e.g., grpcurl)

## Scenario 1: WebSocket Client Receiving Lagged Event

### Setup
1. Start the server on default port (typically 3000 for HTTP, 50051 for gRPC)
2. Open WebSocket connection to `ws://localhost:3000/ws/jobs/test-job-1`
3. Simulate a slow subscriber by:
   - Subscribing but not consuming messages
   - Server publishes events rapidly
   - Broadcast buffer fills up

### Expected Behavior
1. Client receives normal progress events:
   ```json
   {
     "event": "progress",
     "job_id": "test-job-1",
     "data": {"percent": 10, "message": "Starting"},
     "timestamp": "2026-04-25T12:00:00Z"
   }
   ```

2. When buffer overflows, client receives lagged event:
   ```json
   {
     "event": "lagged",
     "job_id": "test-job-1",
     "data": {"dropped": 42},
     "timestamp": "2026-04-25T12:00:05Z"
   }
   ```

3. Connection remains open, more events may follow

### Verification
- ✅ Client receives the lagged event
- ✅ Dropped count is accurate (matches buffer overflow amount)
- ✅ job_id matches the subscription
- ✅ Connection stays open after lagged event

---

## Scenario 2: gRPC Client Receiving Lagged Event

### Setup
1. Start the server on port 50051
2. Connect gRPC client to `localhost:50051`
3. Call `StreamContractEvents()` with a contract filter
4. Simulate slow consumer (similar to scenario 1)

### Expected Behavior
1. Client receives normal `ContractEvent` messages:
   ```protobuf
   {
     ledger_sequence: 0,
     ledger_close_time: 1703511600,
     contract_id: "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
     event_type: "simulation_completed",
     topics_json: "[...]",
     value_json: "{\"cpu_instructions\": 1000000, ...}",
     tx_hash: ""
   }
   ```

2. When buffer overflows, client receives lagged event:
   ```protobuf
   {
     ledger_sequence: 0,
     ledger_close_time: 1703511605,
     contract_id: "",  // empty for meta-events
     event_type: "lagged",  // special event type
     topics_json: "[]",
     value_json: "{\"dropped\": 42}",
     tx_hash: ""
   }
   ```

3. Stream closes gracefully after lagged event

### Verification
- ✅ Client receives the lagged event
- ✅ `event_type` is "lagged"
- ✅ `contract_id` is empty (meta-event)
- ✅ `value_json` contains `{"dropped": N}`
- ✅ Stream closes cleanly after sending lagged event

---

## Scenario 3: Multiple Lagged Events (if applicable)

### Note
In the current implementation, gRPC streams close after the first lagged event. WebSocket connections may continue, but multiple lagged events in sequence would indicate a persistent slow consumer.

---

## Scenario 4: Client Reconnection After Lagged

### WebSocket Steps
1. Client receives lagged event with dropped count
2. Client can:
   - **Continue listening**: Connection remains open, may receive more events from current position
   - **Reconnect**: Close connection and open new one, potentially with `start_ledger` to rewind
   - **Log & Alert**: Log the gap for monitoring

### gRPC Steps
1. Client receives lagged event and stream ends
2. Client can:
   - **Reconnect**: Open new stream, potentially with earlier `start_ledger`
   - **Increase buffer**: Check if client-side buffering helps
   - **Log & Alert**: Log the gap for monitoring

---

## Scenario 5: Normal Operation (No Lag)

### Expected Behavior
1. Fast consumers receive all events without lagged notification
2. No performance degradation
3. No additional overhead for normal operation

### Verification
- ✅ No lagged events when subscriber keeps up
- ✅ All events flow normally
- ✅ No extra messages in logs (only WARN when actual lag occurs)

---

## Load Testing Script (Pseudocode)

```bash
#!/bin/bash

# Start server
cargo run -p sky-moon-scope-core &
SERVER_PID=$!
sleep 2

# Connect WebSocket client and simulate slow consumer
(
  echo 'Connecting to WebSocket...'
  # Use your WebSocket client tool
  websocat ws://localhost:3000/ws/jobs/test-load-1
) &
CLIENT_PID=$!

# Publish events rapidly to trigger overflow
for i in {1..200}; do
  # Trigger job progress updates (server-side)
  curl -X POST http://localhost:3000/api/jobs/test-load-1/progress \
    -H "Content-Type: application/json" \
    -d "{\"percent\": $(($i % 100)), \"message\": \"Tick $i\"}"
  sleep 0.01  # 10ms interval = 100 events/sec
done

# Let it run for a bit more
sleep 5

# Check logs for lagged events
kill $CLIENT_PID $SERVER_PID
```

---

## Logging Expectations

### WebSocket (WARN level)
```
WARN: WebSocket consumer lagged — events were skipped (job_id=test-job-1, skipped=42)
```

### gRPC (WARN level)
```
WARN: gRPC stream lagged; some events were dropped (skipped=42)
```

### Errors (ERROR level, if applicable)
```
ERROR: Failed to serialise Lagged event (job_id=test-job-1, error=...)
ERROR: failed to send lagged event to gRPC stream (error=...)
```

---

## Metrics to Monitor

After deployment, monitor these metrics:

1. **Lagged event count**: Number of times subscribers fell behind
   - Should be rare in normal operation
   - High frequency indicates undersized broadcast buffer or slow consumers

2. **Dropped event totals**: Total events lost due to lag
   - Cumulative of all `dropped` counts
   - Useful for capacity planning

3. **Stream closure rate**: Increased due to graceful closure after lagged events
   - Correlates with lag frequency

---

## Troubleshooting

### Symptom: Client hangs after lagged event

**WebSocket**: Connection should remain open unless explicitly closed
- Check: Socket send doesn't error
- Check: Timestamp is valid

**gRPC**: Stream should close cleanly after lagged event
- Check: No errors in log
- Check: Client can reconnect successfully

### Symptom: No lagged events even when subscriber is slow

**Possible causes**:
- Buffer isn't actually overflowing (increase publication rate)
- Consumer is keeping up better than expected
- Buffer capacity is very large

**Solution**: Reduce buffer capacity for testing or increase publication rate

### Symptom: Lagged event has incorrect dropped count

**Possible causes**:
- Multiple subscribers, other consumed before lag
- Race condition (unlikely, but possible in high-concurrency scenarios)

**Solution**: Check that only one subscriber exists during test

---

## Cleanup After Testing

```bash
# Kill any lingering server processes
pkill -f "cargo run.*sky-moon-scope-core"

# Check for open ports
lsof -i :3000 :50051

# Clear any test data
# (depending on your database setup)
```
