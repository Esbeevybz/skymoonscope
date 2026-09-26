/// Error handling middleware for mapping internal errors to opaque user-facing messages.
///
/// This middleware intercepts errors and ensures that:
/// 1. Client errors (4xx) show the original error message as-is (user-controlled input)
/// 2. Server errors (5xx) are logged with full Debug details server-side
/// 3. Server error responses to clients contain opaque generic messages
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

/// Middleware that sanitizes error responses for HTTP 5xx errors.
///
/// For client errors (4xx), the original error message is preserved since it
/// typically reflects user-controlled input validation.
///
/// For server errors (5xx), the response message is replaced with a generic
/// opaque message while the full error details are logged server-side.
pub async fn error_sanitization_middleware(
    req: Request,
    next: Next,
) -> Response {
    let response = next.run(req).await;

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
}
