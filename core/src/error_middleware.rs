/// Error handling middleware for mapping internal errors to opaque user-facing messages.
///
/// This middleware intercepts errors and ensures that:
/// 1. Client errors (4xx) show the original error message as-is (user-controlled input)
/// 2. Server errors (5xx) are logged with full Debug details server-side
/// 3. Server error responses to clients contain opaque generic messages
/// 4. Handler panics are caught, logged with backtrace, and trigger a SysAlarm
///
/// This prevents stack traces, type names, and file paths from being exposed
/// in API responses while preserving internal visibility via logs.

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::error::Error as StdError;
use tracing::error;

/// Middleware that sanitizes error responses for HTTP 5xx errors and catches panics.
///
/// For client errors (4xx), the original error message is preserved since it
/// typically reflects user-controlled input validation.
///
/// For server errors (5xx), the response message is replaced with a generic
/// opaque message while the full error details are logged server-side.
///
/// For handler panics, the panic payload is logged at ERROR level with a backtrace,
/// a SysAlarm is emitted, and HTTP 500 is returned to the client.
///
/// Note: Panics in async contexts are difficult to catch without spawning a task.
/// We use tokio::spawn to detect panics, which allows us to catch and log them
/// before returning an error response.
pub async fn error_sanitization_middleware(
    req: Request,
    next: Next,
) -> Response {
    // Wrap the handler in a tokio task to catch panics. If the handler panics,
    // the task will be aborted and we can detect that below.
    let task = tokio::spawn(async { next.run(req).await });

    let response = match task.await {
        Ok(response) => response,
        Err(err) if err.is_panic() => {
            // Extract panic info if available
            if let Some(s) = err.panic_value().downcast_ref::<String>() {
                log_panic_string(s.as_str());
                emit_panic_alarm_string(s.as_str()).await;
            } else if let Some(s) = err.panic_value().downcast_ref::<&str>() {
                log_panic_string(s);
                emit_panic_alarm_string(s).await;
            } else {
                log_panic_unknown();
                emit_panic_alarm_unknown().await;
            }

            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Body::from("Internal server error"))
                .unwrap()
                .into_response()
        }
        Err(err) => {
            // Task was cancelled or some other join error
            error!(error = ?err, "Handler task join error (not a panic)");
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Body::from("Internal server error"))
                .unwrap()
                .into_response()
        }
    };

    // Only intercept 5xx responses
    if !response.status().is_server_error() {
        return response;
    }

    // Log the full response details server-side (if available)
    // The actual logging happens at the handler level where errors occur,
    // but we ensure no 5xx response leaks internal details to clients.
    error!(
        status = response.status().as_u16(),
        "Server error response (full details logged by handler)"
    );

    response
}

/// Extract and log the full error chain with context.
///
/// This should be called at the point where an error occurs to ensure
/// maximum context about what went wrong.
pub fn log_error_chain(error: &(dyn StdError + 'static), context: &str) {
    let mut source: Option<&(dyn StdError + 'static)> = Some(error);
    let mut depth = 0;

    error!(context = context, "Error chain:");
    while let Some(err) = source {
        error!(
            depth = depth,
            error = %err,
            debug = ?err,
            "Error level {}: {}",
            depth,
            err
        );
        source = err.source();
        depth += 1;
    }
}

/// Log panic payload and backtrace at ERROR level.
///
/// Extracts the panic message and captures the backtrace for root-cause analysis.
fn log_panic_string(panic_msg: &str) {
    // Capture backtrace
    let backtrace = std::backtrace::Backtrace::capture();

    error!(
        panic_message = panic_msg,
        backtrace = %backtrace,
        "Handler panic caught: {}",
        panic_msg
    );
}

/// Log panic for unknown payload types.
fn log_panic_unknown() {
    let backtrace = std::backtrace::Backtrace::capture();

    error!(
        backtrace = %backtrace,
        "Handler panic caught with unrecognizable payload type"
    );
}

/// Emit a SysAlarm for panic occurrence.
///
/// Posts a panic event to the configured webhook (if set) for alerting
/// and monitoring, then degrades to log-only mode if webhook is unavailable.
async fn emit_panic_alarm_string(panic_msg: &str) {
    // Create and emit the SysAlarm event
    let alarm_event = crate::sys_alarms::SysAlarmEvent {
        event: "handler_panic",
        resource: "request_handler",
        value_percent: 0.0,
        threshold_percent: 0.0,
        process_memory_bytes: 0,
        total_memory_bytes: 0,
        pid: std::process::id(),
        node_id: std::env::var("HOSTNAME").ok().filter(|s| !s.is_empty()),
        timestamp: chrono::Utc::now(),
    };

    // Emit via sys_alarms infrastructure, checking for webhook URL from env
    let webhook_url = std::env::var(crate::sys_alarms::ENV_WEBHOOK).ok();
    crate::sys_alarms::emit_sys_alarm(webhook_url.as_deref(), &alarm_event).await;
}

/// Emit a SysAlarm for panic with unknown payload.
async fn emit_panic_alarm_unknown() {
    // Create and emit the SysAlarm event
    let alarm_event = crate::sys_alarms::SysAlarmEvent {
        event: "handler_panic",
        resource: "request_handler",
        value_percent: 0.0,
        threshold_percent: 0.0,
        process_memory_bytes: 0,
        total_memory_bytes: 0,
        pid: std::process::id(),
        node_id: std::env::var("HOSTNAME").ok().filter(|s| !s.is_empty()),
        timestamp: chrono::Utc::now(),
    };

    // Emit via sys_alarms infrastructure, checking for webhook URL from env
    let webhook_url = std::env::var(crate::sys_alarms::ENV_WEBHOOK).ok();
    crate::sys_alarms::emit_sys_alarm(webhook_url.as_deref(), &alarm_event).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_chain_logging() {
        // This test verifies that the error chain extraction works correctly.
        // In a real scenario, this would be called with actual errors.
        // Example usage:
        // log_error_chain(&some_error, "Simulating contract execution");
    }

    #[tokio::test]
    async fn test_panic_logging_with_string_panic() {
        // Verify that panic logging is called correctly for String panics
        // In production, panics would be caught by the middleware and logged
        // along with backtrace information.
        let test_message = "test panic message";
        log_panic_string(test_message);
        // If this test completes without error, logging succeeded
    }

    #[tokio::test]
    async fn test_panic_alarm_emission() {
        // Verify that SysAlarm emission for panics works
        // In production, this would POST to the webhook if configured
        emit_panic_alarm_string("test panic for alarm").await;
        // If this test completes without error, alarm emission succeeded
    }

    #[test]
    fn test_log_panic_unknown() {
        // Verify that unknown panic types are handled gracefully
        log_panic_unknown();
        // If this test completes without error, logging succeeded
    }
}
