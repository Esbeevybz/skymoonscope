use axum::{
    extract::{Extension, Json, Path},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::json;
use std::sync::Arc;
use tracing::{error, warn};

use crate::shared_results_domain::{
    CreateShareRequest, CreateShareResponse, ShareExistsResponse, SharedResult, ViewShareResponse,
};
use crate::shared_results_store::{generate_share_id, SharedResultsStore, SharedResultsStoreError};

/// Application state for shared results handler
#[derive(Clone)]
pub struct SharedResultsState {
    pub store: Arc<SharedResultsStore>,
    pub base_url: String,
}

/// Create a new shareable link for a profiling result
/// POST /api/shares
pub async fn create_share(
    Extension(state): Extension<SharedResultsState>,
    Json(payload): Json<CreateShareRequest>,
) -> impl IntoResponse {
    // Validate request
    if let Err(e) = payload.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Invalid request",
                "message": e
            })),
        )
            .into_response();
    }

    // Generate share ID
    let share_id = generate_share_id();

    // Create SharedResult
    let result_json = serde_json::to_string(&payload.result).unwrap_or_default();
    let metadata = json!({
        "is_public": payload.is_public,
    });
    let metadata_json = serde_json::to_string(&metadata).ok();

    let shared = SharedResult::new(
        share_id.clone(),
        result_json,
        metadata_json,
        payload.ttl_days,
        None, // TODO: Get from auth context
    );

    // Store in database
    match state.store.create(shared).await {
        Ok(stored) => {
            let response = CreateShareResponse::new(
                stored.share_id,
                &state.base_url,
                stored.expires_at,
            );
            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(e) => {
            error!("Failed to create share: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to create share",
                    "message": e.to_string()
                })),
            )
                .into_response()
        }
    }
}

/// Retrieve a shared profiling result
/// GET /api/shares/:share_id
pub async fn get_share(
    Extension(state): Extension<SharedResultsState>,
    Path(share_id): Path<String>,
) -> impl IntoResponse {
    // Validate share_id format
    if !SharedResult::is_valid_share_id(&share_id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Invalid share ID format"
            })),
        )
            .into_response();
    }

    match state.store.get_by_share_id(&share_id).await {
        Ok(mut shared) => {
            // Increment access count
            if let Err(e) = state.store.increment_access(&share_id).await {
                warn!("Failed to increment access count: {}", e);
            } else {
                shared.increment_access();
            }

            match ViewShareResponse::from_shared_result(shared) {
                Ok(response) => (StatusCode::OK, Json(response)).into_response(),
                Err(e) => {
                    error!("Failed to serialize shared result: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({
                            "error": "Failed to retrieve share",
                            "message": "Invalid data format"
                        })),
                    )
                        .into_response()
                }
            }
        }
        Err(SharedResultsStoreError::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "Share not found"
            })),
        )
            .into_response(),
        Err(SharedResultsStoreError::Expired) => (
            StatusCode::GONE,
            Json(json!({
                "error": "Share has expired"
            })),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to retrieve share: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to retrieve share",
                    "message": e.to_string()
                })),
            )
                .into_response()
        }
    }
}

/// Check if a share exists and is valid
/// GET /api/shares/:share_id/exists
pub async fn check_share_exists(
    Extension(state): Extension<SharedResultsState>,
    Path(share_id): Path<String>,
) -> impl IntoResponse {
    if !SharedResult::is_valid_share_id(&share_id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Invalid share ID format"
            })),
        )
            .into_response();
    }

    match state.store.exists(&share_id).await {
        Ok(exists) => {
            let mut response = ShareExistsResponse {
                exists,
                expires_at: None,
            };

            if exists {
                if let Ok(Some(expiry)) = state.store.get_expiry(&share_id).await {
                    response.expires_at = Some(expiry);
                }
            }

            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            error!("Failed to check share existence: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to check share",
                    "message": e.to_string()
                })),
            )
                .into_response()
        }
    }
}

/// Delete a shared result (owner only - requires auth)
/// DELETE /api/shares/:share_id
pub async fn delete_share(
    Extension(state): Extension<SharedResultsState>,
    Path(share_id): Path<String>,
) -> impl IntoResponse {
    if !SharedResult::is_valid_share_id(&share_id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Invalid share ID format"
            })),
        )
            .into_response();
    }

    match state.store.delete(&share_id).await {
        Ok(()) => (StatusCode::NO_CONTENT).into_response(),
        Err(SharedResultsStoreError::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "Share not found"
            })),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to delete share: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to delete share",
                    "message": e.to_string()
                })),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_share_id() {
        assert!(SharedResult::is_valid_share_id("abc123xyz789"));
        assert!(SharedResult::is_valid_share_id("a".repeat(12).as_str()));
    }

    #[test]
    fn test_invalid_share_id() {
        assert!(!SharedResult::is_valid_share_id("abc")); // Too short
        assert!(!SharedResult::is_valid_share_id("abc-123-xyz")); // Invalid chars
    }
}
