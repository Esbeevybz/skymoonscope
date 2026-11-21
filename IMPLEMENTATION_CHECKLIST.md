# Lagged Event Fix - Implementation Checklist

## Code Changes

### ws.rs Changes
- [x] Added `LaggedPayload` struct with `dropped: u64` field
- [x] Added `Lagged` variant to `SimulationEvent` enum
- [x] Updated `SimulationEvent::job_id()` to handle `Lagged` variant
- [x] Updated `SimulationEvent::is_terminal()` to return `false` for `Lagged`
- [x] Enhanced `handle_socket()` to create and send `Lagged` event on `RecvError::Lagged`
- [x] Added error handling for serialization failures
- [x] Added two unit tests for lagged events

### grpc.rs Changes
- [x] Added `translate_lagged_event(dropped: u64)` helper function
- [x] Enhanced background task to call `translate_lagged_event` on `RecvError::Lagged`
- [x] Added error handling and logging for lagged event send failures
- [x] Break the receive loop after sending lagged event (graceful stream closure)
- [x] Added chrono usage for timestamp generation

## Protocol & Semantics

### WebSocket Protocol
- [x] Lagged event sent as JSON-serialized `SimulationEvent`
- [x] Contains `job_id`, `data.dropped`, and `timestamp`
- [x] Serializes to `{"event":"lagged","job_id":"...","data":{"dropped":N},"timestamp":"..."}`
- [x] Non-terminal event (stream continues after sending)

### gRPC Protocol
- [x] Lagged event synthesized as `ContractEvent` with `event_type: "lagged"`
- [x] `contract_id` empty (meta-event, not associated with a contract)
- [x] `value_json` contains `{"dropped": N}`
- [x] `topics_json` set to empty array `"[]"`
- [x] Timestamps properly set to current time

## Error Handling

- [x] JSON serialization errors logged, stream continues
- [x] Socket send errors cause stream closure
- [x] gRPC channel send errors logged but don't crash
- [x] Lagged event breaks gRPC receive loop for graceful closure
- [x] WebSocket continues accepting events after lagged notification

## Testing & Verification

- [x] Unit tests verify lagged event serialization
- [x] Unit tests verify lagged events are non-terminal
- [x] Tests verify job_id is correctly set in lagged events
- [x] Existing tests for progress/completed/failed events still pass
- [x] Documentation added explaining client behavior

## Code Quality

- [x] Follows existing code style and patterns
- [x] Proper error logging at appropriate levels (WARN for dropped events, ERROR for failures)
- [x] Clear comments explaining meta-event nature
- [x] No breaking changes to existing APIs
- [x] Backward compatible (clients ignoring "lagged" events still work)

## Behavior Verification

### WebSocket Scenario
1. Client connects to `/ws/jobs/{job_id}`
2. Server publishes events at rapid rate, client falls behind
3. Broadcast buffer fills and overflow occurs
4. Next `rx.recv()` returns `RecvError::Lagged(n)`
5. Server creates `Lagged` event with `dropped: n`
6. Server sends JSON frame to WebSocket client
7. Client receives `{"event":"lagged","job_id":"...","data":{"dropped":42},...}`
8. Connection remains open, future events continue flowing
9. **Expected outcome**: Client can detect gap and take action (reconnect/retry)

### gRPC Scenario
1. Client opens streaming RPC `StreamContractEvents()`
2. Server publishes events rapidly, client slow to consume
3. Background task's per-connection queue fills
4. Broadcast channel overflows, `RecvError::Lagged(n)` occurs
5. Server creates synthetic `ContractEvent` with `event_type: "lagged"`
6. Server sends `ContractEvent` to client via gRPC stream
7. Client receives message with `event_type: "lagged"`
8. Stream terminates gracefully after sending lagged event
9. **Expected outcome**: Client can detect gap and decide to reconnect

## Integration Points

- [x] `SimulationBus` still publishes only `SimulationEvent` variants (no runtime changes)
- [x] Lagged events are internal notifications (not published on the bus)
- [x] Existing subscribers won't receive lagged events (correct isolation)
- [x] Both WebSocket and gRPC clients are independently notified

## Documentation

- [x] Protocol documentation updated (LAGGED_EVENT_FIX.md)
- [x] Client behavior explained
- [x] Example JSON/Protobuf structures shown
- [x] Future enhancement ideas documented

## Deployment Considerations

- [x] No database migrations needed
- [x] No configuration changes required
- [x] Backward compatible with older clients
- [x] No new dependencies introduced
- [x] Environment variables unchanged

## Known Limitations / Future Work

- [ ] Metrics/observability: No counters for dropped events yet
- [ ] Client-side retry logic not implemented (client responsibility)
- [ ] No adaptive throttling based on lag
- [ ] Integration tests with artificially slow consumers not added yet
- [ ] No UI changes needed (pure protocol enhancement)
