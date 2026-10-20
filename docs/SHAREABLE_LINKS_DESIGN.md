# Shareable Links for Profiling Results - Design Document

## Overview
This document outlines the design for shareable link generation for profiling results in Sky Moon Scope.

## Design Decisions

### 1. URL Structure
- **Share ID Format**: URL-friendly short IDs using `nanoid` (12-16 characters)
- **URL Pattern**: `https://skymoonscope.app/share/:shareId`
- **Example**: `https://skymoonscope.app/share/abc123xyz789`

### 2. Backend Storage
- **Primary**: SQLite database (existing database in `/core`)
- **Cache**: Optional in-memory cache for frequently accessed shares
- **Fallback**: File system cache for high-traffic scenarios

### 3. Data Model
```
shared_results {
  id: UUID (primary key)
  share_id: String (unique, 12-16 chars)
  result_data: JSON (full profiling result)
  metadata: JSON (contract name, function name, created_at, etc.)
  created_at: DateTime
  expires_at: DateTime (default: +30 days)
  access_count: i32
  created_by: String (optional, for tracking)
}
```

### 4. API Endpoints

#### POST /api/shares
**Create a shareable link**
- Input:
  ```json
  {
    "result": { /* InvocationResult object */ },
    "ttl_days": 30,
    "is_public": true
  }
  ```
- Output:
  ```json
  {
    "share_id": "abc123xyz789",
    "share_url": "https://skymoonscope.app/share/abc123xyz789",
    "expires_at": "2026-11-17T23:07:06.899Z"
  }
  ```

#### GET /api/shares/:shareId
**Retrieve shared profiling result**
- Response:
  ```json
  {
    "share_id": "abc123xyz789",
    "result": { /* InvocationResult */ },
    "metadata": { /* creation info */ },
    "created_at": "2026-10-17T23:07:06.899Z",
    "access_count": 42
  }
  ```

#### GET /api/shares/:shareId/exists
**Check if a share exists and is valid**
- Response: `{ "exists": true, "expires_at": "..." }`

### 5. Security Considerations
- **Rate Limiting**: Max 10 shares per minute per IP
- **Expiration**: Default 30 days, configurable up to 90 days
- **Access Logging**: Track access_count for insights
- **Public/Private**: Optional access control (future enhancement)
- **Data Validation**: Sanitize input, validate result structure
- **HTTPS Only**: Enforce in production

### 6. Frontend UX
- **Share Button**: Add to result viewer (next to export)
- **Share Modal**: Copy link, preview, set expiry
- **Shared View Route**: `/share/[id]` page for viewing shared results
- **Notifications**: Toast on copy success

### 7. Database Migration
- Create `shared_results` table in migration
- Add indexes on `share_id`, `expires_at`, `created_at`
- Include cleanup job for expired shares

## Implementation Phases

### Phase 1: Backend Database & API
1. Create migration for `shared_results` table
2. Implement domain model and repository
3. Add POST /api/shares endpoint
4. Add GET /api/shares/:shareId endpoint
5. Add expiration cleanup job

### Phase 2: Frontend UI
1. Add share button to result viewer
2. Create share modal component
3. Implement /share/[id] page
4. Add share link preview

### Phase 3: Testing & Deployment
1. Unit tests for sharing logic
2. Integration tests for API
3. E2E tests for frontend
4. CI pipeline verification

## Error Handling
- 404: Share not found
- 410: Share expired
- 429: Rate limit exceeded
- 400: Invalid input
- 503: Database unavailable

## Future Enhancements
- Analytics dashboard for shares
- Custom expiry per share
- Password-protected shares
- Social sharing (Twitter, Discord)
- QR code generation
- Share statistics (views, unique visitors)
