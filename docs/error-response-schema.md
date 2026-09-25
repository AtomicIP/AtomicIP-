# Canonical Error Response Schema

## Overview

The Atomic Patent API uses a **canonical error response shape** defined in `api-server/src/response.rs`. All error responses across every endpoint must conform to this standardized structure to ensure consistency, predictability, and client reliability.

## Canonical Error Response Structure

Every error response follows this exact JSON shape:

```json
{
  "status": 400,
  "message": "Human-readable error description",
  "data": null,
  "error": {
    "code": "ERROR_CODE",
    "message": "Detailed error message",
    "details": {
      "field_name": ["Error message 1", "Error message 2"],
      "other_field": ["Error message"]
    }
  },
  "meta": {
    "request_id": "550e8400-e29b-41d4-a716-446655440000",
    "timestamp": 1234567890,
    "version": "1.0.0"
  }
}
```

### Field Definitions

#### `status` (required, integer)
- HTTP status code (400, 401, 403, 404, 409, 500, etc.)
- Must be ≥ 400 for error responses
- Matches the HTTP response status code

#### `message` (required, string)
- Human-readable error summary (2-100 characters)
- Should be concise and actionable
- Example: "Validation failed"

#### `data` (optional, null)
- **Must always be `null` for error responses**
- Never populated in error cases
- Omitted in serialization when null

#### `error` (required, object)
##### `error.code` (required, string)
- Programmatic error identifier
- Format: `UPPERCASE_SNAKE_CASE`
- Examples: `UNAUTHORIZED`, `NOT_FOUND`, `VALIDATION_ERROR`, `INTERNAL_ERROR`
- Used by clients to handle errors programmatically

##### `error.message` (required, string)
- Detailed explanation of the error
- Can be longer than top-level `message`
- Provides context for debugging

##### `error.details` (optional, object<string, string[]>)
- Field-level validation errors only
- Map of field names to arrays of error messages
- Only included for 400-level validation errors
- Example: `{"email": ["Email is required", "Email must be valid"], "age": ["Age must be >= 18"]}`

#### `meta` (required, object)
##### `meta.request_id` (string)
- Unique request identifier for tracing
- UUID v4 format
- Helps correlate logs across systems

##### `meta.timestamp` (integer)
- Unix timestamp of response
- Seconds since epoch

##### `meta.version` (string)
- API version (e.g., "1.0.0")
- Identifies which API version generated the response

## Error Response Types and Codes

### 400 Bad Request

**HTTP Status**: 400  
**Error Code**: `INVALID_INPUT` or `VALIDATION_ERROR`  
**Use Case**: Request is malformed or validation fails

```rust
ResponseFormatter::bad_request("Invalid input", "INVALID_INPUT")
ResponseFormatter::bad_request_with_details("Validation failed", "VALIDATION_ERROR", details_map)
```

**Example**:
```json
{
  "status": 400,
  "message": "Validation failed",
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "One or more fields failed validation",
    "details": {
      "commitment_hash": ["Must be 32 bytes", "Must be hex-encoded"]
    }
  },
  "meta": { ... }
}
```

### 401 Unauthorized

**HTTP Status**: 401  
**Error Code**: `UNAUTHORIZED`  
**Use Case**: Request lacks valid authentication credentials

```rust
ResponseFormatter::unauthorized("Not authenticated")
```

**Example**:
```json
{
  "status": 401,
  "message": "Not authenticated",
  "error": {
    "code": "UNAUTHORIZED",
    "message": "Request must include valid API key or bearer token"
  },
  "meta": { ... }
}
```

### 403 Forbidden

**HTTP Status**: 403  
**Error Code**: `FORBIDDEN`  
**Use Case**: Authenticated but lacks permission

```rust
ResponseFormatter::forbidden("Access denied")
```

**Example**:
```json
{
  "status": 403,
  "message": "Access denied",
  "error": {
    "code": "FORBIDDEN",
    "message": "User does not have permission to perform this action"
  },
  "meta": { ... }
}
```

### 404 Not Found

**HTTP Status**: 404  
**Error Code**: `NOT_FOUND`  
**Use Case**: Requested resource does not exist

```rust
ResponseFormatter::not_found("IP record not found")
```

**Example**:
```json
{
  "status": 404,
  "message": "IP record not found",
  "error": {
    "code": "NOT_FOUND",
    "message": "IP with ID 12345 does not exist"
  },
  "meta": { ... }
}
```

### 409 Conflict

**HTTP Status**: 409  
**Error Code**: `CONFLICT`  
**Use Case**: Request conflicts with current state (duplicate, already revoked, etc.)

```rust
ResponseFormatter::conflict("IP already revoked")
```

**Example**:
```json
{
  "status": 409,
  "message": "IP already revoked",
  "error": {
    "code": "CONFLICT",
    "message": "Cannot transfer a revoked IP"
  },
  "meta": { ... }
}
```

### 500 Internal Server Error

**HTTP Status**: 500  
**Error Code**: `INTERNAL_ERROR`  
**Use Case**: Unexpected server-side error

```rust
ResponseFormatter::internal_error("Internal server error")
```

**Example**:
```json
{
  "status": 500,
  "message": "Internal server error",
  "error": {
    "code": "INTERNAL_ERROR",
    "message": "Failed to connect to Soroban RPC. Request ID: abc123..."
  },
  "meta": { ... }
}
```

## Enforcement and Consistency

### All Handlers Must Use ResponseFormatter

**Rule**: Every error response in every handler **must** be created using `ResponseFormatter` methods.

**Allowed**:
```rust
// ✓ Correct: Using ResponseFormatter
Err(ResponseFormatter::not_found("IP not found"))
ResponseFormatter::bad_request_with_details("Validation failed", "VALIDATION_ERROR", details)
```

**Not Allowed**:
```rust
// ✗ Wrong: Hand-constructed error response
Err(json!({
    "status": 404,
    "message": "Not found",
    // Missing error.code, error.message, meta, etc.
}))

// ✗ Wrong: Inconsistent structure
Err(StatusCode::NOT_FOUND, "Not found")
```

### Handler Audit Checklist

When adding a new handler:
- [ ] All error paths use `ResponseFormatter` methods
- [ ] No hand-constructed error JSON
- [ ] Error codes are `UPPERCASE_SNAKE_CASE`
- [ ] Validation errors include `details` map
- [ ] All responses include `meta.request_id` and `meta.timestamp`

### Automated Consistency Tests

The test suite in `api-server/src/response.rs` verifies:

1. **Shape Consistency**: All error responses have canonical structure
2. **Field Validation**: Required fields are never empty
3. **Status Code Ranges**: 400-level and 500-level codes are used appropriately
4. **Code Format**: Error codes follow `UPPERCASE_SNAKE_CASE`
5. **No Data Pollution**: Error responses never populate the `data` field
6. **Metadata Presence**: All responses include complete `meta` object
7. **Details Structure**: Field-level errors use correct format (array of strings)

**Running Tests**:
```bash
cd api-server
cargo test response::tests -- --nocapture
cargo test --test '*' response -- --nocapture
```

## Client-Side Handling

Clients should handle errors programmatically using `error.code`:

```typescript
// TypeScript Example
try {
  const response = await client.getIp(ipId);
} catch (err) {
  const error = err.error;
  
  switch (error.code) {
    case 'NOT_FOUND':
      // Handle missing resource
      break;
    case 'FORBIDDEN':
      // Handle permission denied
      break;
    case 'VALIDATION_ERROR':
      // Display field-level errors from error.details
      for (const [field, messages] of Object.entries(error.details)) {
        console.error(`${field}: ${messages.join(', ')}`);
      }
      break;
    default:
      // Generic error handling
      console.error(error.message);
  }
}
```

## Extensibility

### Adding a New Error Code

1. Create a new `ResponseFormatter` method if the error pattern is reusable
2. Document the error code and when it's used in this document
3. Use the new method consistently across all relevant handlers
4. Add a test case to verify the error response shape

Example:
```rust
// In response.rs
pub fn rate_limited(message: impl Into<String>) -> ApiResponse<()> {
    ApiResponse {
        status: 429,
        message: message.into(),
        data: None,
        error: Some(ErrorDetails {
            code: "RATE_LIMITED".to_string(),
            message: "Request rate limit exceeded".to_string(),
            details: None,
        }),
        meta: Self::create_meta(),
    }
}

// In test
#[test]
fn test_rate_limited_error_response() {
    let response = ResponseFormatter::rate_limited("Slow down");
    assert_eq!(response.status, 429);
    assert_eq!(response.error.unwrap().code, "RATE_LIMITED");
}
```

## References

- **Response Formatter Implementation**: `api-server/src/response.rs`
- **Consistency Tests**: `api-server/src/response.rs::tests`
- **HTTP Status Codes**: https://httpwg.org/specs/rfc7231.html#status.codes
- **Error Code Best Practices**: https://tools.ietf.org/html/draft-ietf-appsawg-http-problem#section-3.1
