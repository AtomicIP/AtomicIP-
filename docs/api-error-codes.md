# API Error Codes

This document describes all error codes returned by the Atomic Patent API and provides recovery suggestions for each.

## HTTP Status Codes

### 400 Bad Request

Returned when the request is malformed or contains invalid data.

**Common Causes:**
- Missing required fields in request body
- Invalid field format (e.g., non-hex commitment hash)
- Array length mismatch in batch operations
- Empty arrays in batch requests

**Recovery:**
1. Validate all required fields are present
2. Ensure hex strings are properly formatted (64 characters for 32-byte values)
3. For batch operations, verify all arrays have the same length
4. Check the error message for specific field validation details

**Example Error Response:**
```json
{
  "error": "ip_ids and prices must have the same length"
}
```

### 401 Unauthorized

Returned when authentication fails or credentials are invalid.

**Common Causes:**
- Missing or invalid `Authorization` header
- Expired or revoked API key
- Invalid signature for signed requests

**Recovery:**
1. Verify the `Authorization` header is present
2. Check that your API key is valid and not expired
3. For signed requests, ensure the signature is computed correctly
4. Regenerate API credentials if necessary

### 404 Not Found

Returned when a requested resource does not exist.

**Common Causes:**
- IP record ID does not exist
- Swap record ID does not exist
- Webhook ID does not exist

**Recovery:**
1. Verify the resource ID is correct
2. Check that the resource has not been deleted
3. Use list endpoints to discover valid resource IDs
4. Ensure you're querying the correct network (testnet vs mainnet)

**Example Error Response:**
```json
{
  "error": "IP record 12345 not found"
}
```

### 409 Conflict

Returned when the request conflicts with the current state of a resource.

**Common Causes:**
- Attempting to commit a duplicate commitment hash
- Attempting to transfer IP that is already in a swap
- Attempting to accept a swap that is not in Pending state
- Attempting to reveal key for a swap that is not in Accepted state

**Recovery:**
1. Check the current state of the resource using GET endpoints
2. Ensure the operation is valid for the current state
3. Wait for pending operations to complete before retrying
4. For duplicate commitments, use a different secret or blinding factor

**Example Error Response:**
```json
{
  "error": "Commitment hash already exists"
}
```

### 422 Unprocessable Entity

Returned when the request is valid but cannot be processed due to business logic constraints.

**Common Causes:**
- Insufficient balance for swap payment
- Invalid Stellar address format
- Swap expiry time is in the past
- Commitment hash verification failed

**Recovery:**
1. Verify all business logic constraints are met
2. For payment issues, ensure sufficient balance in the token account
3. Validate Stellar addresses using the Stellar SDK
4. Ensure expiry times are in the future
5. For verification failures, check the secret and blinding factor

**Example Error Response:**
```json
{
  "error": "Invalid Stellar address format"
}
```

### 429 Too Many Requests

Returned when rate limit is exceeded.

**Common Causes:**
- Exceeding API rate limit (typically 100 requests per minute per API key)
- Burst traffic exceeding per-second limits

**Recovery:**
1. Implement exponential backoff in your client
2. Reduce request frequency
3. Batch operations where possible using bulk endpoints
4. Contact support if you need higher rate limits

**Example Error Response:**
```json
{
  "error": "Rate limit exceeded. Retry after 60 seconds"
}
```

### 500 Internal Server Error

Returned when an unexpected server error occurs.

**Common Causes:**
- Soroban RPC connection failure
- Database connection failure
- Unexpected exception in handler

**Recovery:**
1. Retry the request after a short delay (exponential backoff)
2. Check the API status page for known issues
3. Contact support if the error persists
4. Include the request ID (from response headers) when reporting

**Example Error Response:**
```json
{
  "error": "Internal server error. Request ID: abc123def456"
}
```

### 503 Service Unavailable

Returned when the API is temporarily unavailable.

**Common Causes:**
- Scheduled maintenance
- Soroban network issues
- Database maintenance

**Recovery:**
1. Wait and retry after a delay
2. Check the API status page for maintenance windows
3. Implement retry logic with exponential backoff
4. Subscribe to status updates

## Error Response Format

All error responses follow this format:

```json
{
  "error": "Human-readable error message",
  "code": "ERROR_CODE",
  "details": {
    "field": "field_name",
    "reason": "Specific validation failure reason"
  },
  "request_id": "unique-request-identifier"
}
```

## Common Error Scenarios

### Commitment Hash Validation

**Error:** `"Invalid commitment hash format"`
- **Cause:** Commitment hash is not a valid 64-character hex string
- **Fix:** Ensure hash is 32 bytes (64 hex characters)

### Stellar Address Validation

**Error:** `"Invalid Stellar address"`
- **Cause:** Address does not start with 'G' or is not 56 characters
- **Fix:** Use valid Stellar public key format

### Batch Operation Validation

**Error:** `"ip_ids and prices must have the same length"`
- **Cause:** Arrays in batch request have different lengths
- **Fix:** Ensure all arrays have matching lengths

### Swap State Validation

**Error:** `"Swap is not in Accepted state"`
- **Cause:** Attempting to reveal key for swap not in Accepted state
- **Fix:** Check swap status before attempting operation

## Testing Error Handling

### Unit Tests

```rust
#[tokio::test]
async fn test_commit_ip_invalid_hash_returns_400() {
    let app = build_app();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/ip/commit")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"owner":"GADDR","commitment_hash":"invalid"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
```

### Integration Tests

Test error scenarios against a test network:

```bash
# Test invalid commitment hash
curl -X POST http://localhost:8080/v1/ip/commit \
  -H "Content-Type: application/json" \
  -d '{"owner":"GADDR","commitment_hash":"invalid"}'

# Test missing required field
curl -X POST http://localhost:8080/v1/ip/commit \
  -H "Content-Type: application/json" \
  -d '{"owner":"GADDR"}'

# Test batch operation with mismatched arrays
curl -X POST http://localhost:8080/v1/swap/bulk/initiate \
  -H "Content-Type: application/json" \
  -d '{"ip_ids":[1,2],"prices":[100]}'
```

## Best Practices

1. **Always check HTTP status code** before parsing response body
2. **Implement exponential backoff** for 5xx errors and 429 responses
3. **Log error details** including request ID for debugging
4. **Validate input** before sending to API to catch errors early
5. **Handle timeouts** separately from other errors
6. **Use request IDs** when contacting support

## Support

For issues not covered in this documentation:
- Check the [API Reference](api-reference.md)
- Review [Integration Guide](integration-guide.md)
- Contact support with your request ID
