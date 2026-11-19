# Shareable Links Feature - Implementation Guide

## Overview

This document describes the implementation of shareable links for profiling results in Sky Moon Scope. This feature allows users to generate and share links to specific profiling analysis results without requiring the viewer to run their own analysis.

## Features

- **Generate Shareable Links**: Users can create short, unique URLs that point to specific profiling results
- **Configurable Expiration**: Links expire after a configurable period (1-90 days, default 30 days)
- **Access Tracking**: Track how many times each shared result has been viewed
- **Public/Private**: Support for future enhancements like access control
- **Read-Only Sharing**: Shared results are read-only to prevent accidental modifications

## Architecture

### Backend (Rust)

#### Database Schema
- **Table**: `shared_results`
- **Fields**:
  - `id` (TEXT, PRIMARY KEY): UUID identifier
  - `share_id` (TEXT, UNIQUE): Short URL-friendly ID (12-16 alphanumeric chars)
  - `result_data` (TEXT): Full profiling result JSON
  - `metadata` (TEXT, NULLABLE): Additional metadata JSON
  - `created_at` (TEXT): ISO 8601 timestamp
  - `expires_at` (TEXT): Expiration timestamp
  - `access_count` (INTEGER): Number of views
  - `created_by` (TEXT, NULLABLE): Creator identifier
  - `updated_at` (TEXT): Last update timestamp

**Indexes**:
- `share_id`: For fast lookup by share ID
- `expires_at`: For efficient cleanup of expired shares
- `created_at`: For chronological ordering
- `created_by`: For user-specific queries

#### API Endpoints

##### POST /api/shares
Create a new shareable link

**Request Body**:
```json
{
  "result": { /* profiling result object */ },
  "ttl_days": 30,
  "is_public": true
}
```

**Response** (201 Created):
```json
{
  "share_id": "abc123xyz789",
  "share_url": "https://skymoonscope.app/share/abc123xyz789",
  "expires_at": "2026-11-17T23:07:06.899Z"
}
```

**Error Codes**:
- `400`: Invalid request (invalid TTL, malformed result)
- `500`: Server error

##### GET /api/shares/:share_id
Retrieve a shared profiling result

**Response** (200 OK):
```json
{
  "share_id": "abc123xyz789",
  "result": { /* profiling result */ },
  "metadata": { /* metadata if available */ },
  "created_at": "2026-10-17T23:07:06.899Z",
  "access_count": 42
}
```

**Error Codes**:
- `400`: Invalid share ID format
- `404`: Share not found
- `410`: Share expired
- `500`: Server error

##### GET /api/shares/:share_id/exists
Check if a share exists and is valid

**Response** (200 OK):
```json
{
  "exists": true,
  "expires_at": "2026-11-17T23:07:06.899Z"
}
```

##### DELETE /api/shares/:share_id
Delete a shared result (future: requires auth)

**Response** (204 No Content)

**Error Codes**:
- `404`: Share not found
- `500`: Server error

#### Rust Modules

##### `shared_results_domain.rs`
Domain models and DTOs:
- `SharedResult`: Main domain model
- `CreateShareRequest`: Request DTO
- `CreateShareResponse`: Response DTO
- `ViewShareResponse`: View response DTO
- `ShareExistsResponse`: Existence check response

##### `shared_results_store.rs`
Data access layer (repository pattern):
- `SharedResultsStore`: Main store interface
- `SharedResultsStoreError`: Error types
- `generate_share_id()`: ID generation function

Methods:
- `create()`: Insert new shared result
- `get_by_share_id()`: Retrieve by ID
- `exists()`: Check existence
- `get_expiry()`: Get expiration time
- `delete()`: Remove a share
- `delete_expired()`: Cleanup expired shares
- `increment_access()`: Track views
- `get_by_creator()`: List user's shares

##### `shared_results_handlers.rs`
HTTP request handlers:
- `create_share()`: POST handler
- `get_share()`: GET handler
- `check_share_exists()`: Existence check handler
- `delete_share()`: DELETE handler

### Frontend (Next.js / TypeScript)

#### Components

##### `ShareButton.tsx`
Component for generating and managing share links

**Props**:
```typescript
interface ShareButtonProps {
  resultId?: string;
  functionName?: string;
  data: any;
  onShareSuccess?: (shareUrl: string) => void;
}
```

**Features**:
- Modal dialog for share creation
- TTL configuration (1-90 days)
- Real-time API feedback
- Copy-to-clipboard functionality
- Error handling and feedback

#### Pages

##### `/share/[id].tsx`
Read-only viewer for shared results

**Features**:
- Displays shared profiling results
- Shows creation date and access count
- Renders NutritionLabel component
- Handles expired/not found scenarios
- Responsive design with proper error messages

#### API Client

##### `shareApi.ts`
TypeScript API client for share operations

**Methods**:
```typescript
class ShareApi {
  static async createShare(result, ttlDays?, isPublic?): Promise<ShareResponse>
  static async getShare(shareId): Promise<ViewShareResponse>
  static async checkShareExists(shareId): Promise<ShareExistsResponse>
  static async deleteShare(shareId): Promise<void>
  static generateShareUrl(shareId): string
  static async copyToClipboard(url): Promise<void>
}
```

## Database Migrations

### Migration: 003_create_shared_results_table.sql

Executed during server startup via `sqlx migrate!()`. Creates:
1. `shared_results` table with all necessary fields
2. Indexes for performance optimization
3. Cleanup indexes for expired records

## Security Considerations

1. **Rate Limiting**: Implement at application level to prevent abuse
   - Max 10 shares per minute per IP (configurable)
   
2. **Expiration**: Default 30 days, maximum 90 days
   - Automatic cleanup of expired shares via background job
   
3. **Data Validation**:
   - Share ID format validation (alphanumeric, 12-16 chars)
   - TTL range validation (1-90 days)
   - Result data structure validation
   
4. **HTTPS Only**: Enforce in production environments
   
5. **Access Control** (future):
   - Optional password protection
   - Optional authentication requirement
   - Creator-only deletion

## Testing

### Backend Tests (`core/tests/shared_results.rs`)

Unit tests covering:
- Share ID generation and format validation
- Expiration logic
- TTL validation (1-90 days)
- Request validation
- Database operations
- Error handling

### Frontend Tests (`web/lib/shareApi.test.cjs`)

Integration tests covering:
- Share button rendering and interaction
- API communication
- URL generation and copying
- Error handling
- Loading states
- Data serialization/deserialization

## Usage Examples

### Creating a Share Link (Frontend)

```typescript
import { ShareApi } from './lib/shareApi';

// In your component
const profiling Result = { /* result data */ };

const response = await ShareApi.createShare(
  profilingResult,
  30,  // expires in 30 days
  true // is public
);

console.log('Share URL:', response.share_url);
// Output: Share URL: https://skymoonscope.app/share/abc123xyz789
```

### Retrieving a Shared Result (Frontend)

```typescript
const shareId = 'abc123xyz789';

try {
  const shared = await ShareApi.getShare(shareId);
  console.log('Result:', shared.result);
  console.log('Views:', shared.access_count);
} catch (error) {
  if (error.message.includes('expired')) {
    console.log('Share has expired');
  } else if (error.message.includes('not found')) {
    console.log('Share not found');
  }
}
```

### Backend Usage (Rust)

```rust
use crate::shared_results_store::{SharedResultsStore, generate_share_id};
use crate::shared_results_domain::SharedResult;

// Create a shared result
let shared = SharedResult::new(
    generate_share_id(),
    result_json,
    Some(metadata_json),
    30,  // TTL in days
    Some("user-id".to_string())
);

// Store it
let stored = store.create(shared).await?;

// Retrieve it later
let retrieved = store.get_by_share_id(&stored.share_id).await?;
```

## Deployment Considerations

1. **Database Migration**: Ensure migrations run during deployment
   - `sqlx migrate run` or automatic via `sqlx::migrate!()`

2. **Cleanup Job**: Background task to delete expired shares
   - Configure retention policy
   - Set cleanup frequency

3. **Environment Variables**:
   - `DATABASE_URL`: Connection string
   - `BASE_URL`: For generating share URLs

4. **Monitoring**:
   - Track share creation rate
   - Monitor access patterns
   - Alert on errors

## Future Enhancements

1. **Access Control**:
   - Password-protected shares
   - Authentication-required shares
   - Share expiration on first access

2. **Analytics**:
   - Per-share view statistics
   - Unique visitor tracking
   - Referrer tracking

3. **Social Sharing**:
   - Pre-filled social media posts
   - QR code generation
   - Email sharing

4. **Advanced Features**:
   - Share collections (multiple results)
   - Custom expiration per share
   - Notification on share access

5. **Performance**:
   - Redis caching for frequent shares
   - CDN-friendly share links
   - Compression of large results

## Troubleshooting

### Share URL Returns 404
- Verify share ID format (12-16 alphanumeric)
- Check if share has expired
- Ensure database migrations have run

### Slow Share Creation
- Check database performance
- Consider adding indexes
- Monitor network latency

### High Memory Usage
- Implement result compression
- Add pagination for large results
- Consider streaming responses

## References

- Design Document: `/docs/SHAREABLE_LINKS_DESIGN.md`
- Database Migration: `/core/migrations/003_create_shared_results_table.sql`
- Backend Implementation: `/core/src/shared_results_*`
- Frontend Implementation: `/web/components/ShareButton.tsx`, `/web/pages/share/[id].tsx`
