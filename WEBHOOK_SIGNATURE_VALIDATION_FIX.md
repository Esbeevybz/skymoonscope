# Webhook Signature Validation Security Fix

## Vulnerability Description

The `ContractSubscription` struct had a security vulnerability in its validation logic:

### The Problem
- The `signing_secret` field was validated in the `ContractSubscription::new()` constructor to ensure it has at least 32 bytes (required for HMAC-SHA256 signatures)
- However, the struct derived `Deserialize` directly, which means HTTP endpoints accepting JSON payloads (e.g., PATCH requests) could bypass this constructor guard
- An attacker could send a PATCH request with `"signing_secret": ""` and serde would deserialize it directly without validation
- This would create a subscription with an empty signing secret, making HMAC signatures trivially forgeable

### Attack Vector
```bash
PATCH /api/subscriptions/{id}
Content-Type: application/json

{
  "signing_secret": ""
}
```

This would bypass the 32-byte minimum requirement and store an empty key.

## The Fix

Implemented a custom `Deserialize` trait that enforces all validation rules during deserialization:

### Changes Made to `core/src/webhooks.rs`:

1. **Removed automatic derive**:
   - Changed from: `#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]`
   - To manual implementation of `Deserialize` trait

2. **Implemented custom `Deserialize`**:
   - Validates `signing_secret` length (>= 32 bytes)
   - Validates `contract_id` is not empty
   - Validates `callback_url` uses http or https
   - Returns `serde::de::Error` on validation failure

3. **Added comprehensive tests**:
   - `deserialization_rejects_signing_secret_shorter_than_32_bytes()` — empty string case
   - `deserialization_rejects_short_signing_secret()` — undersized secret case
   - `deserialization_accepts_valid_signing_secret()` — valid case
   - `deserialization_rejects_invalid_contract_id()` — empty contract_id case
   - `deserialization_rejects_invalid_callback_url()` — invalid protocol case

## Implementation Details

```rust
impl<'de> Deserialize<'de> for ContractSubscription {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct SubscriptionHelper {
            #[serde(default)]
            id: Option<Uuid>,
            contract_id: String,
            #[serde(default)]
            event_types: Vec<String>,
            callback_url: String,
            signing_secret: String,
            #[serde(default = "default_active")]
            active: bool,
        }

        let helper = SubscriptionHelper::deserialize(deserializer)?;

        // Validate signing_secret length
        if helper.signing_secret.len() < 32 {
            return Err(serde::de::Error::custom(
                "signing_secret must contain at least 32 bytes",
            ));
        }

        // Validate contract_id and callback_url similarly...

        Ok(ContractSubscription {
            id: helper.id.unwrap_or_else(Uuid::new_v4),
            contract_id: helper.contract_id,
            event_types: helper.event_types,
            callback_url: helper.callback_url,
            signing_secret: helper.signing_secret,
            active: helper.active,
        })
    }
}
```

## Defense-in-Depth

The fix implements validation at multiple layers:

1. **Constructor validation** (`ContractSubscription::new()`): Still validates all rules for programmatic creation
2. **Deserialization validation** (custom impl): Now validates all rules for HTTP-based updates
3. **Tests**: Verify both paths enforce the rules

This ensures the vulnerability cannot be bypassed whether subscriptions are created:
- Programmatically via the constructor
- Via HTTP JSON deserialization (PATCH/PUT endpoints)
- Via any other serialization mechanism

## Impact

- ✅ Prevents empty or weak signing secrets from being stored
- ✅ Ensures HMAC signatures remain cryptographically secure
- ✅ Blocks forgeability attacks that relied on weak secrets
- ✅ Maintains backward compatibility with valid existing subscriptions
- ✅ Provides clear error messages for validation failures

## Testing

All existing tests pass plus 5 new tests specifically validate:
1. Empty signing_secret rejection
2. Short signing_secret rejection
3. Valid signing_secret acceptance
4. Invalid contract_id rejection
5. Invalid callback_url rejection

## Files Modified

- `/workspaces/skymoonscope/core/src/webhooks.rs`
  - Implemented custom `Deserialize` for `ContractSubscription`
  - Added 5 comprehensive validation tests
