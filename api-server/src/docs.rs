use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// API documentation generator
pub struct ApiDocGenerator;

/// Endpoint documentation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EndpointDoc {
    /// HTTP method (GET, POST, etc.)
    pub method: String,
    /// API path
    pub path: String,
    /// Endpoint summary
    pub summary: String,
    /// Detailed description
    pub description: String,
    /// Request parameters
    pub parameters: Vec<ParameterDoc>,
    /// Request body schema
    pub request_body: Option<SchemaDoc>,
    /// Response schemas by status code
    pub responses: HashMap<u16, ResponseDoc>,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Whether authentication is required
    pub requires_auth: bool,
    /// Rate limit info
    pub rate_limit: Option<RateLimitDoc>,
}

/// Parameter documentation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ParameterDoc {
    /// Parameter name
    pub name: String,
    /// Parameter location (path, query, header)
    pub location: String,
    /// Parameter description
    pub description: String,
    /// Data type
    pub data_type: String,
    /// Whether parameter is required
    pub required: bool,
    /// Example value
    pub example: Option<String>,
}

/// Schema documentation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SchemaDoc {
    /// Schema name
    pub name: String,
    /// Schema description
    pub description: String,
    /// Schema properties
    pub properties: HashMap<String, PropertyDoc>,
    /// Required properties
    pub required: Vec<String>,
}

/// Property documentation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PropertyDoc {
    /// Property type
    pub data_type: String,
    /// Property description
    pub description: String,
    /// Example value
    pub example: Option<String>,
}

/// Response documentation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResponseDoc {
    /// HTTP status code
    pub status: u16,
    /// Response description
    pub description: String,
    /// Response schema
    pub schema: Option<SchemaDoc>,
}

/// Rate limit documentation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RateLimitDoc {
    /// Requests per time window
    pub requests: u32,
    /// Time window in seconds
    pub window_seconds: u32,
}

/// Complete API documentation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiDocumentation {
    /// API title
    pub title: String,
    /// API version
    pub version: String,
    /// API description
    pub description: String,
    /// Base URL
    pub base_url: String,
    /// All endpoints
    pub endpoints: Vec<EndpointDoc>,
    /// Authentication schemes
    pub auth_schemes: Vec<AuthSchemeDoc>,
    /// Common error codes
    pub error_codes: HashMap<String, String>,
}

/// Authentication scheme documentation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthSchemeDoc {
    /// Scheme name
    pub name: String,
    /// Scheme type (Bearer, ApiKey, etc.)
    pub scheme_type: String,
    /// Description
    pub description: String,
}

impl ApiDocGenerator {
    /// Generate documentation for IP Registry endpoints
    pub fn ip_registry_docs() -> Vec<EndpointDoc> {
        vec![
            EndpointDoc {
                method: "POST".to_string(),
                path: "/v1/ip/commit".to_string(),
                summary: "Commit a new IP".to_string(),
                description: "Timestamp a new IP commitment with a Pedersen hash. Returns the assigned IP ID.".to_string(),
                parameters: vec![],
                request_body: Some(SchemaDoc {
                    name: "CommitIpRequest".to_string(),
                    description: "IP commitment request".to_string(),
                    properties: [
                        ("owner".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "Stellar address of the IP owner".to_string(),
                            example: Some("GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZD".to_string()),
                        }),
                        ("commitment_hash".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "32-byte Pedersen commitment hash, hex-encoded".to_string(),
                            example: Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string()),
                        }),
                    ].iter().cloned().collect(),
                    required: vec!["owner".to_string(), "commitment_hash".to_string()],
                }),
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "IP committed successfully, returns assigned ip_id".to_string(),
                        schema: None,
                    }),
                    (400, ResponseDoc {
                        status: 400,
                        description: "Invalid request (zero hash, duplicate hash)".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["IP Registry".to_string()],
                requires_auth: true,
                rate_limit: Some(RateLimitDoc {
                    requests: 100,
                    window_seconds: 60,
                }),
            },
            EndpointDoc {
                method: "GET".to_string(),
                path: "/v1/ip/{ip_id}".to_string(),
                summary: "Retrieve an IP record".to_string(),
                description: "Get details of an IP record by ID.".to_string(),
                parameters: vec![
                    ParameterDoc {
                        name: "ip_id".to_string(),
                        location: "path".to_string(),
                        description: "IP record identifier".to_string(),
                        data_type: "integer".to_string(),
                        required: true,
                        example: Some("1".to_string()),
                    },
                ],
                request_body: None,
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "IP record found".to_string(),
                        schema: None,
                    }),
                    (404, ResponseDoc {
                        status: 404,
                        description: "IP record not found".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["IP Registry".to_string()],
                requires_auth: false,
                rate_limit: Some(RateLimitDoc {
                    requests: 1000,
                    window_seconds: 60,
                }),
            },
            EndpointDoc {
                method: "POST".to_string(),
                path: "/v1/ip/transfer".to_string(),
                summary: "Transfer IP ownership".to_string(),
                description: "Transfer IP ownership to a new address.".to_string(),
                parameters: vec![],
                request_body: Some(SchemaDoc {
                    name: "TransferIpRequest".to_string(),
                    description: "Transfer IP ownership request".to_string(),
                    properties: [
                        ("ip_id".to_string(), PropertyDoc {
                            data_type: "integer".to_string(),
                            description: "IP record identifier".to_string(),
                            example: Some("1".to_string()),
                        }),
                        ("new_owner".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "New owner's Stellar address".to_string(),
                            example: Some("GDEF456...".to_string()),
                        }),
                    ].iter().cloned().collect(),
                    required: vec!["ip_id".to_string(), "new_owner".to_string()],
                }),
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "Ownership transferred successfully".to_string(),
                        schema: None,
                    }),
                    (404, ResponseDoc {
                        status: 404,
                        description: "IP record not found".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["IP Registry".to_string()],
                requires_auth: true,
                rate_limit: Some(RateLimitDoc {
                    requests: 100,
                    window_seconds: 60,
                }),
            },
            EndpointDoc {
                method: "POST".to_string(),
                path: "/v1/ip/verify".to_string(),
                summary: "Verify commitment".to_string(),
                description: "Verify a Pedersen commitment: sha256(secret || blinding_factor) == commitment_hash.".to_string(),
                parameters: vec![],
                request_body: Some(SchemaDoc {
                    name: "VerifyCommitmentRequest".to_string(),
                    description: "Verify commitment request".to_string(),
                    properties: [
                        ("ip_id".to_string(), PropertyDoc {
                            data_type: "integer".to_string(),
                            description: "IP record identifier".to_string(),
                            example: Some("1".to_string()),
                        }),
                        ("secret".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "32-byte secret hex".to_string(),
                            example: Some("e3b0c442...".to_string()),
                        }),
                        ("blinding_factor".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "32-byte blinding factor hex".to_string(),
                            example: Some("5feceb66...".to_string()),
                        }),
                    ].iter().cloned().collect(),
                    required: vec!["ip_id".to_string(), "secret".to_string(), "blinding_factor".to_string()],
                }),
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "Verification result".to_string(),
                        schema: None,
                    }),
                    (404, ResponseDoc {
                        status: 404,
                        description: "IP record not found".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["IP Registry".to_string()],
                requires_auth: false,
                rate_limit: Some(RateLimitDoc {
                    requests: 500,
                    window_seconds: 60,
                }),
            },
            EndpointDoc {
                method: "GET".to_string(),
                path: "/v1/ip/owner/{owner}".to_string(),
                summary: "List IPs by owner".to_string(),
                description: "List all IP IDs owned by a Stellar address with offset pagination support.".to_string(),
                parameters: vec![
                    ParameterDoc {
                        name: "owner".to_string(),
                        location: "path".to_string(),
                        description: "Stellar address of the owner".to_string(),
                        data_type: "string".to_string(),
                        required: true,
                        example: Some("GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZD".to_string()),
                    },
                    ParameterDoc {
                        name: "limit".to_string(),
                        location: "query".to_string(),
                        description: "Maximum number of items to return (default: 50, max: 200)".to_string(),
                        data_type: "integer".to_string(),
                        required: false,
                        example: Some("50".to_string()),
                    },
                    ParameterDoc {
                        name: "offset".to_string(),
                        location: "query".to_string(),
                        description: "Number of items to skip (default: 0)".to_string(),
                        data_type: "integer".to_string(),
                        required: false,
                        example: Some("0".to_string()),
                    },
                ],
                request_body: None,
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "Paginated list of IP IDs".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["IP Registry".to_string()],
                requires_auth: false,
                rate_limit: Some(RateLimitDoc {
                    requests: 500,
                    window_seconds: 60,
                }),
            },
            EndpointDoc {
                method: "GET".to_string(),
                path: "/v1/ip/owner/{owner}/cursor".to_string(),
                summary: "List IPs by owner with cursor pagination".to_string(),
                description: "List all IP IDs owned by a Stellar address with cursor-based pagination.".to_string(),
                parameters: vec![
                    ParameterDoc {
                        name: "owner".to_string(),
                        location: "path".to_string(),
                        description: "Stellar address of the owner".to_string(),
                        data_type: "string".to_string(),
                        required: true,
                        example: Some("GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZD".to_string()),
                    },
                    ParameterDoc {
                        name: "limit".to_string(),
                        location: "query".to_string(),
                        description: "Maximum number of items to return (default: 50, max: 200)".to_string(),
                        data_type: "integer".to_string(),
                        required: false,
                        example: Some("50".to_string()),
                    },
                    ParameterDoc {
                        name: "cursor".to_string(),
                        location: "query".to_string(),
                        description: "Opaque cursor token for next page".to_string(),
                        data_type: "string".to_string(),
                        required: false,
                        example: Some("eyJ...".to_string()),
                    },
                ],
                request_body: None,
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "Cursor-paginated list of IP IDs".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["IP Registry".to_string()],
                requires_auth: false,
                rate_limit: Some(RateLimitDoc {
                    requests: 500,
                    window_seconds: 60,
                }),
            },
            EndpointDoc {
                method: "POST".to_string(),
                path: "/v1/bulk/commit-ip".to_string(),
                summary: "Bulk commit IP records".to_string(),
                description: "Commit multiple IP records in a single batch request.".to_string(),
                parameters: vec![],
                request_body: Some(SchemaDoc {
                    name: "BulkCommitIpRequest".to_string(),
                    description: "Bulk commit request".to_string(),
                    properties: [
                        ("owner".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "Owner address".to_string(),
                            example: Some("GABC...".to_string()),
                        }),
                        ("commitment_hashes".to_string(), PropertyDoc {
                            data_type: "array".to_string(),
                            description: "List of hex commitment hashes".to_string(),
                            example: Some("[\"hash1\", \"hash2\"]".to_string()),
                        }),
                    ].iter().cloned().collect(),
                    required: vec!["owner".to_string(), "commitment_hashes".to_string()],
                }),
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "Bulk commit completed with individual results".to_string(),
                        schema: None,
                    }),
                    (400, ResponseDoc {
                        status: 400,
                        description: "Invalid request (empty array, etc.)".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["IP Registry".to_string()],
                requires_auth: true,
                rate_limit: Some(RateLimitDoc {
                    requests: 50,
                    window_seconds: 60,
                }),
            },
        ]
    }

    /// Generate documentation for Atomic Swap endpoints
    pub fn atomic_swap_docs() -> Vec<EndpointDoc> {
        vec![
            EndpointDoc {
                method: "POST".to_string(),
                path: "/v1/swap/initiate".to_string(),
                summary: "Initiate a patent sale".to_string(),
                description: "Seller initiates a patent sale. Returns the swap ID.".to_string(),
                parameters: vec![],
                request_body: Some(SchemaDoc {
                    name: "InitiateSwapRequest".to_string(),
                    description: "Swap initiation request".to_string(),
                    properties: [
                        ("ip_id".to_string(), PropertyDoc {
                            data_type: "integer".to_string(),
                            description: "IP record identifier".to_string(),
                            example: Some("1".to_string()),
                        }),
                        ("seller".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "Seller's Stellar address".to_string(),
                            example: Some("GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZD".to_string()),
                        }),
                        ("price".to_string(), PropertyDoc {
                            data_type: "integer".to_string(),
                            description: "Price in stroops".to_string(),
                            example: Some("1000000".to_string()),
                        }),
                        ("buyer".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "Buyer's Stellar address".to_string(),
                            example: Some("GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZD".to_string()),
                        }),
                    ].iter().cloned().collect(),
                    required: vec!["ip_id".to_string(), "seller".to_string(), "price".to_string(), "buyer".to_string()],
                }),
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "Swap initiated successfully, returns swap_id".to_string(),
                        schema: None,
                    }),
                    (400, ResponseDoc {
                        status: 400,
                        description: "Seller is not IP owner or active swap exists".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["Atomic Swap".to_string()],
                requires_auth: true,
                rate_limit: Some(RateLimitDoc {
                    requests: 100,
                    window_seconds: 60,
                }),
            },
            EndpointDoc {
                method: "POST".to_string(),
                path: "/v1/swap/batch-initiate".to_string(),
                summary: "Batch initiate patent sales".to_string(),
                description: "Seller initiates multiple patent sales in one call. Returns a list of swap IDs.".to_string(),
                parameters: vec![],
                request_body: Some(SchemaDoc {
                    name: "BatchInitiateSwapRequest".to_string(),
                    description: "Batch swap initiation request".to_string(),
                    properties: [
                        ("ip_registry_id".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "IP registry contract ID".to_string(),
                            example: Some("C1...".to_string()),
                        }),
                        ("ip_ids".to_string(), PropertyDoc {
                            data_type: "array".to_string(),
                            description: "Vector of IP IDs".to_string(),
                            example: Some("[1, 2, 3]".to_string()),
                        }),
                        ("seller".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "Seller address".to_string(),
                            example: Some("G1...".to_string()),
                        }),
                        ("prices".to_string(), PropertyDoc {
                            data_type: "array".to_string(),
                            description: "Prices in stroops".to_string(),
                            example: Some("[100, 200, 300]".to_string()),
                        }),
                        ("buyer".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "Buyer address".to_string(),
                            example: Some("G2...".to_string()),
                        }),
                        ("token".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "Token address".to_string(),
                            example: Some("C2...".to_string()),
                        }),
                    ].iter().cloned().collect(),
                    required: vec!["ip_registry_id".to_string(), "ip_ids".to_string(), "seller".to_string(), "prices".to_string(), "buyer".to_string(), "token".to_string()],
                }),
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "Swaps initiated, returns swap_ids".to_string(),
                        schema: None,
                    }),
                    (400, ResponseDoc {
                        status: 400,
                        description: "Validation error (mismatched lengths, etc.)".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["Atomic Swap".to_string()],
                requires_auth: true,
                rate_limit: Some(RateLimitDoc {
                    requests: 50,
                    window_seconds: 60,
                }),
            },
            EndpointDoc {
                method: "POST".to_string(),
                path: "/v1/swap/{swap_id}/accept".to_string(),
                summary: "Accept a swap".to_string(),
                description: "Buyer accepts a pending swap and locks payment in escrow.".to_string(),
                parameters: vec![
                    ParameterDoc {
                        name: "swap_id".to_string(),
                        location: "path".to_string(),
                        description: "Swap identifier".to_string(),
                        data_type: "integer".to_string(),
                        required: true,
                        example: Some("1".to_string()),
                    },
                ],
                request_body: Some(SchemaDoc {
                    name: "AcceptSwapRequest".to_string(),
                    description: "Accept swap request".to_string(),
                    properties: [
                        ("buyer".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "Buyer's address".to_string(),
                            example: Some("GBUYER...".to_string()),
                        }),
                    ].iter().cloned().collect(),
                    required: vec!["buyer".to_string()],
                }),
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "Swap accepted".to_string(),
                        schema: None,
                    }),
                    (400, ResponseDoc {
                        status: 400,
                        description: "Swap not in Pending state".to_string(),
                        schema: None,
                    }),
                    (404, ResponseDoc {
                        status: 404,
                        description: "Swap not found".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["Atomic Swap".to_string()],
                requires_auth: true,
                rate_limit: Some(RateLimitDoc {
                    requests: 100,
                    window_seconds: 60,
                }),
            },
            EndpointDoc {
                method: "POST".to_string(),
                path: "/v1/swap/{swap_id}/reveal".to_string(),
                summary: "Reveal decryption key".to_string(),
                description: "Seller reveals the decryption key; payment releases and swap completes.".to_string(),
                parameters: vec![
                    ParameterDoc {
                        name: "swap_id".to_string(),
                        location: "path".to_string(),
                        description: "Swap identifier".to_string(),
                        data_type: "integer".to_string(),
                        required: true,
                        example: Some("1".to_string()),
                    },
                ],
                request_body: Some(SchemaDoc {
                    name: "RevealKeyRequest".to_string(),
                    description: "Reveal key request".to_string(),
                    properties: [
                        ("seller".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "Seller address".to_string(),
                            example: Some("GSELLER...".to_string()),
                        }),
                        ("decryption_key".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "Decryption key hex".to_string(),
                            example: Some("a1b2c3...".to_string()),
                        }),
                    ].iter().cloned().collect(),
                    required: vec!["seller".to_string(), "decryption_key".to_string()],
                }),
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "Key revealed, swap completed".to_string(),
                        schema: None,
                    }),
                    (400, ResponseDoc {
                        status: 400,
                        description: "Swap not in Accepted state or caller is not seller".to_string(),
                        schema: None,
                    }),
                    (404, ResponseDoc {
                        status: 404,
                        description: "Swap not found".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["Atomic Swap".to_string()],
                requires_auth: true,
                rate_limit: Some(RateLimitDoc {
                    requests: 100,
                    window_seconds: 60,
                }),
            },
            EndpointDoc {
                method: "POST".to_string(),
                path: "/v1/swap/{swap_id}/cancel".to_string(),
                summary: "Cancel a pending swap".to_string(),
                description: "Cancel a pending swap. Only the seller or buyer may cancel.".to_string(),
                parameters: vec![
                    ParameterDoc {
                        name: "swap_id".to_string(),
                        location: "path".to_string(),
                        description: "Swap identifier".to_string(),
                        data_type: "integer".to_string(),
                        required: true,
                        example: Some("1".to_string()),
                    },
                ],
                request_body: Some(SchemaDoc {
                    name: "CancelSwapRequest".to_string(),
                    description: "Cancel swap request".to_string(),
                    properties: [
                        ("caller".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "Caller address".to_string(),
                            example: Some("G...".to_string()),
                        }),
                    ].iter().cloned().collect(),
                    required: vec!["caller".to_string()],
                }),
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "Swap cancelled".to_string(),
                        schema: None,
                    }),
                    (400, ResponseDoc {
                        status: 400,
                        description: "Swap not in Pending state".to_string(),
                        schema: None,
                    }),
                    (404, ResponseDoc {
                        status: 404,
                        description: "Swap not found".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["Atomic Swap".to_string()],
                requires_auth: true,
                rate_limit: Some(RateLimitDoc {
                    requests: 100,
                    window_seconds: 60,
                }),
            },
            EndpointDoc {
                method: "POST".to_string(),
                path: "/v1/swap/{swap_id}/cancel-expired".to_string(),
                summary: "Cancel an expired accepted swap".to_string(),
                description: "Buyer cancels an Accepted swap after the expiry timestamp.".to_string(),
                parameters: vec![
                    ParameterDoc {
                        name: "swap_id".to_string(),
                        location: "path".to_string(),
                        description: "Swap identifier".to_string(),
                        data_type: "integer".to_string(),
                        required: true,
                        example: Some("1".to_string()),
                    },
                ],
                request_body: Some(SchemaDoc {
                    name: "CancelExpiredSwapRequest".to_string(),
                    description: "Cancel expired swap request".to_string(),
                    properties: [
                        ("buyer".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "Buyer address".to_string(),
                            example: Some("GBUYER...".to_string()),
                        }),
                    ].iter().cloned().collect(),
                    required: vec!["buyer".to_string()],
                }),
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "Expired swap cancelled".to_string(),
                        schema: None,
                    }),
                    (400, ResponseDoc {
                        status: 400,
                        description: "Swap not expired or caller is not buyer".to_string(),
                        schema: None,
                    }),
                    (404, ResponseDoc {
                        status: 404,
                        description: "Swap not found".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["Atomic Swap".to_string()],
                requires_auth: true,
                rate_limit: Some(RateLimitDoc {
                    requests: 100,
                    window_seconds: 60,
                }),
            },
            EndpointDoc {
                method: "GET".to_string(),
                path: "/v1/swap/{swap_id}".to_string(),
                summary: "Get swap status".to_string(),
                description: "Retrieve a swap record by ID.".to_string(),
                parameters: vec![
                    ParameterDoc {
                        name: "swap_id".to_string(),
                        location: "path".to_string(),
                        description: "Swap identifier".to_string(),
                        data_type: "integer".to_string(),
                        required: true,
                        example: Some("1".to_string()),
                    },
                ],
                request_body: None,
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "Swap record found".to_string(),
                        schema: None,
                    }),
                    (404, ResponseDoc {
                        status: 404,
                        description: "Swap not found".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["Atomic Swap".to_string()],
                requires_auth: false,
                rate_limit: Some(RateLimitDoc {
                    requests: 1000,
                    window_seconds: 60,
                }),
            },
            EndpointDoc {
                method: "POST".to_string(),
                path: "/v1/bulk/initiate-swap".to_string(),
                summary: "Bulk initiate swaps".to_string(),
                description: "Initiate multiple swaps in a single request with individual results.".to_string(),
                parameters: vec![],
                request_body: Some(SchemaDoc {
                    name: "BulkInitiateSwapRequest".to_string(),
                    description: "Bulk swap initiation request".to_string(),
                    properties: [
                        ("ip_registry_id".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "IP registry contract ID".to_string(),
                            example: Some("C1...".to_string()),
                        }),
                        ("ip_ids".to_string(), PropertyDoc {
                            data_type: "array".to_string(),
                            description: "IP IDs".to_string(),
                            example: Some("[1, 2]".to_string()),
                        }),
                        ("seller".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "Seller address".to_string(),
                            example: Some("G1...".to_string()),
                        }),
                        ("prices".to_string(), PropertyDoc {
                            data_type: "array".to_string(),
                            description: "Prices".to_string(),
                            example: Some("[100, 200]".to_string()),
                        }),
                        ("buyer".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "Buyer address".to_string(),
                            example: Some("G2...".to_string()),
                        }),
                        ("token".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "Token address".to_string(),
                            example: Some("C2...".to_string()),
                        }),
                    ].iter().cloned().collect(),
                    required: vec!["ip_registry_id".to_string(), "ip_ids".to_string(), "seller".to_string(), "prices".to_string(), "buyer".to_string(), "token".to_string()],
                }),
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "Bulk swap initiation completed with individual results".to_string(),
                        schema: None,
                    }),
                    (400, ResponseDoc {
                        status: 400,
                        description: "Validation error".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["Atomic Swap".to_string()],
                requires_auth: true,
                rate_limit: Some(RateLimitDoc {
                    requests: 50,
                    window_seconds: 60,
                }),
            },
        ]
    }

    /// Generate documentation for Webhook endpoints
    pub fn webhook_docs() -> Vec<EndpointDoc> {
        vec![
            EndpointDoc {
                method: "POST".to_string(),
                path: "/v1/webhooks".to_string(),
                summary: "Register webhook".to_string(),
                description: "Register a webhook URL to receive swap event notifications.".to_string(),
                parameters: vec![],
                request_body: Some(SchemaDoc {
                    name: "RegisterWebhookRequest".to_string(),
                    description: "Webhook registration request".to_string(),
                    properties: [
                        ("url".to_string(), PropertyDoc {
                            data_type: "string".to_string(),
                            description: "Destination URL".to_string(),
                            example: Some("https://example.com/webhook".to_string()),
                        }),
                        ("events".to_string(), PropertyDoc {
                            data_type: "array".to_string(),
                            description: "Event topics to subscribe to".to_string(),
                            example: Some("[\"swap.status_changed\"]".to_string()),
                        }),
                    ].iter().cloned().collect(),
                    required: vec!["url".to_string(), "events".to_string()],
                }),
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "Webhook registered".to_string(),
                        schema: None,
                    }),
                    (400, ResponseDoc {
                        status: 400,
                        description: "Invalid request".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["Webhooks".to_string()],
                requires_auth: true,
                rate_limit: Some(RateLimitDoc {
                    requests: 20,
                    window_seconds: 60,
                }),
            },
            EndpointDoc {
                method: "DELETE".to_string(),
                path: "/v1/webhooks/{id}".to_string(),
                summary: "Unregister webhook".to_string(),
                description: "Unregister a webhook by UUID.".to_string(),
                parameters: vec![
                    ParameterDoc {
                        name: "id".to_string(),
                        location: "path".to_string(),
                        description: "Webhook UUID".to_string(),
                        data_type: "string".to_string(),
                        required: true,
                        example: Some("550e8400-e29b-41d4-a716-446655440000".to_string()),
                    },
                ],
                request_body: None,
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "Webhook unregistered".to_string(),
                        schema: None,
                    }),
                    (404, ResponseDoc {
                        status: 404,
                        description: "Webhook not found".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["Webhooks".to_string()],
                requires_auth: true,
                rate_limit: Some(RateLimitDoc {
                    requests: 50,
                    window_seconds: 60,
                }),
            },
        ]
    }

    /// Generate documentation for Batch and Event endpoints
    pub fn batch_and_event_docs() -> Vec<EndpointDoc> {
        vec![
            EndpointDoc {
                method: "POST".to_string(),
                path: "/batch".to_string(),
                summary: "Execute batch operations".to_string(),
                description: "Execute multiple sub-requests in a single HTTP POST request.".to_string(),
                parameters: vec![],
                request_body: Some(SchemaDoc {
                    name: "BatchRequest".to_string(),
                    description: "Batch request envelope".to_string(),
                    properties: [
                        ("requests".to_string(), PropertyDoc {
                            data_type: "array".to_string(),
                            description: "List of sub-requests to execute".to_string(),
                            example: None,
                        }),
                    ].iter().cloned().collect(),
                    required: vec!["requests".to_string()],
                }),
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "Batch operations completed".to_string(),
                        schema: None,
                    }),
                    (400, ResponseDoc {
                        status: 400,
                        description: "Invalid batch request".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["Batch".to_string()],
                requires_auth: false,
                rate_limit: Some(RateLimitDoc {
                    requests: 100,
                    window_seconds: 60,
                }),
            },
            EndpointDoc {
                method: "GET".to_string(),
                path: "/events".to_string(),
                summary: "Subscribe to contract SSE events".to_string(),
                description: "Server-Sent Events stream for real-time contract events.".to_string(),
                parameters: vec![],
                request_body: None,
                responses: [
                    (200, ResponseDoc {
                        status: 200,
                        description: "Event stream established".to_string(),
                        schema: None,
                    }),
                ].iter().cloned().collect(),
                tags: vec!["Events".to_string()],
                requires_auth: false,
                rate_limit: Some(RateLimitDoc {
                    requests: 50,
                    window_seconds: 60,
                }),
            },
        ]
    }

    /// Generate complete API documentation
    pub fn generate_full_documentation() -> ApiDocumentation {
        let mut endpoints = Self::ip_registry_docs();
        endpoints.extend(Self::atomic_swap_docs());
        endpoints.extend(Self::webhook_docs());
        endpoints.extend(Self::batch_and_event_docs());

        ApiDocumentation {
            title: "Atomic Patent API".to_string(),
            version: "1.0.0".to_string(),
            description: "Machine-readable specification for the Atomic Patent Soroban smart contract interface.".to_string(),
            base_url: "https://api.atomicpatent.io".to_string(),
            endpoints,
            auth_schemes: vec![
                AuthSchemeDoc {
                    name: "Stellar Signature".to_string(),
                    scheme_type: "Signature".to_string(),
                    description: "Sign requests with Stellar keypair".to_string(),
                },
                AuthSchemeDoc {
                    name: "Bearer Token".to_string(),
                    scheme_type: "Bearer".to_string(),
                    description: "Include JWT token in Authorization header".to_string(),
                },
            ],
            error_codes: [
                ("INVALID_INPUT".to_string(), "Request validation failed".to_string()),
                ("NOT_FOUND".to_string(), "Resource not found".to_string()),
                ("UNAUTHORIZED".to_string(), "Authentication required".to_string()),
                ("FORBIDDEN".to_string(), "Insufficient permissions".to_string()),
                ("CONFLICT".to_string(), "Resource conflict".to_string()),
                ("INTERNAL_ERROR".to_string(), "Server error".to_string()),
            ].iter().cloned().collect(),
        }
    }

    /// Export documentation as JSON
    pub fn export_json() -> Result<String, serde_json::Error> {
        let docs = Self::generate_full_documentation();
        serde_json::to_string_pretty(&docs)
    }

    /// Export documentation as Markdown
    pub fn export_markdown() -> String {
        let docs = Self::generate_full_documentation();
        let mut md = format!("# {}\n\n", docs.title);
        md.push_str(&format!("**Version:** {}\n\n", docs.version));
        md.push_str(&format!("{}\n\n", docs.description));
        md.push_str(&format!("**Base URL:** {}\n\n", docs.base_url));

        md.push_str("## Authentication\n\n");
        for scheme in &docs.auth_schemes {
            md.push_str(&format!("- **{}** ({}): {}\n", scheme.name, scheme.scheme_type, scheme.description));
        }
        md.push_str("\n");

        md.push_str("## Endpoints\n\n");
        for endpoint in &docs.endpoints {
            md.push_str(&format!("### {} {}\n\n", endpoint.method, endpoint.path));
            md.push_str(&format!("**Summary:** {}\n\n", endpoint.summary));
            md.push_str(&format!("{}\n\n", endpoint.description));

            if !endpoint.parameters.is_empty() {
                md.push_str("**Parameters:**\n\n");
                for param in &endpoint.parameters {
                    md.push_str(&format!(
                        "- `{}` ({}, {}): {} {}\n",
                        param.name,
                        param.location,
                        param.data_type,
                        param.description,
                        if param.required { "(required)" } else { "(optional)" }
                    ));
                }
                md.push_str("\n");
            }

            if endpoint.requires_auth {
                md.push_str("**Authentication:** Required\n\n");
            }

            if let Some(rate_limit) = &endpoint.rate_limit {
                md.push_str(&format!(
                    "**Rate Limit:** {} requests per {} seconds\n\n",
                    rate_limit.requests, rate_limit.window_seconds
                ));
            }

            md.push_str("**Responses:**\n\n");
            for (_, response) in &endpoint.responses {
                md.push_str(&format!("- `{}`: {}\n", response.status, response.description));
            }
            md.push_str("\n---\n\n");
        }

        md.push_str("## Error Codes\n\n");
        for (code, description) in &docs.error_codes {
            md.push_str(&format!("- `{}`: {}\n", code, description));
        }

        md
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_registry_docs_not_empty() {
        let docs = ApiDocGenerator::ip_registry_docs();
        assert_eq!(docs.len(), 7);
    }

    #[test]
    fn test_atomic_swap_docs_not_empty() {
        let docs = ApiDocGenerator::atomic_swap_docs();
        assert_eq!(docs.len(), 8);
    }

    #[test]
    fn test_webhook_docs_not_empty() {
        let docs = ApiDocGenerator::webhook_docs();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn test_batch_and_event_docs_not_empty() {
        let docs = ApiDocGenerator::batch_and_event_docs();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn test_full_documentation_generation() {
        let docs = ApiDocGenerator::generate_full_documentation();
        assert_eq!(docs.title, "Atomic Patent API");
        assert_eq!(docs.endpoints.len(), 19);
        assert!(!docs.auth_schemes.is_empty());
        assert!(!docs.error_codes.is_empty());
    }

    #[test]
    fn test_export_json() {
        let json = ApiDocGenerator::export_json();
        assert!(json.is_ok());
        let json_str = json.unwrap();
        assert!(json_str.contains("Atomic Patent API"));
        assert!(json_str.contains("/v1/swap/batch-initiate"));
        assert!(json_str.contains("/v1/bulk/commit-ip"));
    }

    #[test]
    fn test_export_markdown() {
        let md = ApiDocGenerator::export_markdown();
        assert!(md.contains("# Atomic Patent API"));
        assert!(md.contains("## Endpoints"));
        assert!(md.contains("## Authentication"));
        assert!(md.contains("/v1/ip/commit"));
        assert!(md.contains("/v1/swap/{swap_id}/reveal"));
    }

    #[test]
    fn test_all_endpoints_have_valid_methods_and_paths() {
        let docs = ApiDocGenerator::generate_full_documentation();
        for ep in &docs.endpoints {
            assert!(matches!(ep.method.as_str(), "GET" | "POST" | "DELETE" | "PUT" | "PATCH"));
            assert!(ep.path.starts_with('/') || ep.path.starts_with("/v1/"));
            assert!(!ep.summary.is_empty());
            assert!(!ep.description.is_empty());
            assert!(!ep.responses.is_empty());
        }
    }
}

