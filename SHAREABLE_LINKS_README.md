# Shareable Links Feature

## Quick Overview

This is a complete implementation of shareable link generation for Soroban contract profiling results in Sky Moon Scope.

## What's New

Users can now:
1. **Generate shareable links** to profiling results with configurable expiration (1-90 days)
2. **Share results** without requiring viewers to run their own analysis
3. **Track views** with access counting on each shared result
4. **Copy links** to clipboard with one click

## Quick Start

### Generating a Share Link

1. Run a profiling analysis in Sky Moon Scope
2. Click the **Share** button in the result viewer
3. Configure expiration time (1-90 days)
4. Click **Generate Share Link**
5. Copy the URL and share with others

### Viewing a Shared Result

Visit the share URL: `https://skymoonscope.app/share/{shareId}`

The shared result displays:
- Full profiling data (read-only)
- Nutrition label summary
- Creation date
- View count
- Contract and function info (if available)

## API Endpoints

| Method | Endpoint | Purpose |
|--------|----------|---------|
| POST | `/api/shares` | Create a shareable link |
| GET | `/api/shares/:shareId` | Retrieve shared result |
| GET | `/api/shares/:shareId/exists` | Check if share is valid |
| DELETE | `/api/shares/:shareId` | Delete a share |

### Example: Create a Share

```bash
curl -X POST http://localhost:8080/api/shares \
  -H "Content-Type: application/json" \
  -d '{
    "result": { "functionName": "test", "cpuInstructions": 1000 },
    "ttl_days": 30,
    "is_public": true
  }'
```

Response:
```json
{
  "share_id": "abc123xyz789",
  "share_url": "https://skymoonscope.app/share/abc123xyz789",
  "expires_at": "2026-11-17T23:07:06.899Z"
}
```

## Files & Documentation

### Implementation Files
- **Backend**: `core/src/shared_results_*.rs` - Rust implementation
- **Frontend**: `web/components/ShareButton.tsx` - React component
- **Frontend**: `web/pages/share/[id].tsx` - Shared result viewer
- **Frontend**: `web/lib/shareApi.ts` - TypeScript API client

### Documentation
- **Design**: `docs/SHAREABLE_LINKS_DESIGN.md` - Architecture & decisions
- **Implementation**: `docs/SHAREABLE_LINKS_IMPLEMENTATION.md` - Complete guide
- **Checklist**: `IMPLEMENTATION_CHECKLIST.md` - Verification checklist
- **Summary**: `SHAREABLE_LINKS_IMPLEMENTATION_SUMMARY.md` - Overview

### Tests
- **Backend**: `core/tests/shared_results.rs` - Unit tests
- **Frontend**: `web/lib/shareApi.test.cjs` - Integration tests

## Key Features

✅ **Unique Share IDs**: 12-16 character alphanumeric IDs  
✅ **Configurable Expiry**: 1-90 days (default: 30)  
✅ **Access Tracking**: Count views per shared result  
✅ **Full Result Storage**: Complete profiling data preserved  
✅ **Metadata Support**: Contract, function, and creation info  
✅ **Error Handling**: Proper HTTP codes (404, 410, 500)  
✅ **Read-Only**: Viewers cannot modify shared results  
✅ **Responsive UI**: Works on mobile and desktop  

## Database

Uses the existing SQLite database with a new `shared_results` table:

```sql
CREATE TABLE shared_results (
  id TEXT PRIMARY KEY,
  share_id TEXT UNIQUE,
  result_data TEXT,
  metadata TEXT,
  created_at TEXT,
  expires_at TEXT,
  access_count INTEGER,
  created_by TEXT,
  updated_at TEXT
)
```

Indexes on `share_id`, `expires_at`, `created_at`, `created_by` for performance.

## Security

- ✅ Share IDs are cryptographically generated (no predictable patterns)
- ✅ Expiration is strictly enforced (returns 410 Gone)
- ✅ Input validation on all parameters
- ✅ HTTPS enforced in production
- ✅ WASM bytecode excluded from storage

## Deployment

1. **Database**: Migration runs automatically on startup
2. **Routes**: All 4 endpoints registered in the Axum router
3. **Dependencies**: `nanoid` crate added for ID generation
4. **Environment**: No new environment variables required

### Configuration
- Share expiration: Configurable via request (1-90 days)
- Base URL: Inferred from server port
- Database: Uses existing connection pool

## Monitoring

Track these metrics:
- Share creation rate
- Access per shared result
- Expiration patterns
- Error rates by endpoint

## Future Enhancements

See `docs/SHAREABLE_LINKS_DESIGN.md#Future Enhancements` for:
- Rate limiting improvements
- Access control enhancements
- Social sharing integrations
- Analytics dashboards
- QR code generation

## Support

For implementation details, see:
- **Full docs**: `docs/SHAREABLE_LINKS_IMPLEMENTATION.md`
- **API spec**: Section "API Endpoints"
- **Examples**: Section "Usage Examples"

## Status

✅ **COMPLETE** - Ready for production deployment

All acceptance criteria met:
- ✅ Implementation complete and reviewed
- ✅ Tests written and passing
- ✅ Documentation updated
- ✅ CI pipeline green
