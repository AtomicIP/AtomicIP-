use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// SDK code generator for multiple languages
pub struct SdkGenerator;

/// Generated SDK code
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GeneratedSdk {
    /// Programming language
    pub language: String,
    /// Generated code
    pub code: String,
    /// Package/module name
    pub package_name: String,
    /// Version
    pub version: String,
}

/// SDK configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SdkConfig {
    /// Base API URL
    pub base_url: String,
    /// API version
    pub api_version: String,
    /// Package name
    pub package_name: String,
    /// Package version
    pub package_version: String,
}

impl SdkGenerator {
    /// Generate TypeScript/JavaScript SDK
    pub fn generate_typescript(config: &SdkConfig) -> GeneratedSdk {
        let code = format!(
            r#"/**
 * Atomic Patent API Client
 * Auto-generated from OpenAPI specification
 * 
 * @package {package_name}
 * @version {version}
 */

export interface ApiResponse<T> {{
  status: number;
  message: string;
  data?: T;
  error?: ErrorDetails;
  meta: ResponseMeta;
}}

export interface ErrorDetails {{
  code: string;
  message: string;
  details?: Record<string, string[]>;
}}

export interface ResponseMeta {{
  request_id: string;
  timestamp: number;
  version: string;
}}

export interface CommitIpRequest {{
  owner: string;
  commitment_hash: string;
}}

export interface IpRecord {{
  ip_id: number;
  owner: string;
  commitment_hash: string;
  timestamp: number;
  revoked: boolean;
}}

export interface InitiateSwapRequest {{
  ip_registry_id: string;
  ip_id: number;
  seller: string;
  price: number;
  buyer: string;
  token: string;
  referrer?: string;
}}

export interface SwapRecord {{
  ip_id: number;
  ip_registry_id: string;
  seller: string;
  buyer: string;
  price: number;
  token: string;
  status: 'Pending' | 'Accepted' | 'Completed' | 'Cancelled';
  expiry: number;
}}

export class AtomicPatentClient {{
  private baseUrl: string;
  private apiVersion: string;
  private headers: Record<string, string>;

  constructor(baseUrl: string = '{base_url}', apiVersion: string = '{api_version}') {{
    this.baseUrl = baseUrl;
    this.apiVersion = apiVersion;
    this.headers = {{
      'Content-Type': 'application/json',
      'Accept': 'application/json',
    }};
  }}

  /**
   * Commit a new IP
   */
  async commitIp(request: CommitIpRequest): Promise<ApiResponse<number>> {{
    return this.post('/v1/ip/commit', request);
  }}

  /**
   * Get an IP record by ID
   */
  async getIp(ipId: number): Promise<ApiResponse<IpRecord>> {{
    return this.get(`/v1/ip/${{ipId}}`);
  }}

  /**
   * List IPs by owner
   */
  async listIpByOwner(owner: string, limit: number = 50, offset: number = 0): Promise<ApiResponse<any>> {{
    return this.get(`/v1/ip/owner/${{owner}}?limit=${{limit}}&offset=${{offset}}`);
  }}

  /**
   * Initiate a swap
   */
  async initiateSwap(request: InitiateSwapRequest): Promise<ApiResponse<number>> {{
    return this.post('/v1/swap/initiate', request);
  }}

  /**
   * Get a swap record
   */
  async getSwap(swapId: number): Promise<ApiResponse<SwapRecord>> {{
    return this.get(`/v1/swap/${{swapId}}`);
  }}

  /**
   * Accept a swap
   */
  async acceptSwap(swapId: number, buyer: string): Promise<ApiResponse<void>> {{
    return this.post(`/v1/swap/${{swapId}}/accept`, {{ buyer }});
  }}

  /**
   * Reveal decryption key
   */
  async revealKey(swapId: number, caller: string, secret: string, blindingFactor: string): Promise<ApiResponse<void>> {{
    return this.post(`/v1/swap/${{swapId}}/reveal`, {{
      caller,
      secret,
      blinding_factor: blindingFactor,
    }});
  }}

  /**
   * Cancel a swap
   */
  async cancelSwap(swapId: number, canceller: string): Promise<ApiResponse<void>> {{
    return this.post(`/v1/swap/${{swapId}}/cancel`, {{ canceller }});
  }}

  private async get<T>(path: string): Promise<ApiResponse<T>> {{
    const response = await fetch(`${{this.baseUrl}}${{path}}`, {{
      method: 'GET',
      headers: this.headers,
    }});
    return response.json();
  }}

  private async post<T>(path: string, body: any): Promise<ApiResponse<T>> {{
    const response = await fetch(`${{this.baseUrl}}${{path}}`, {{
      method: 'POST',
      headers: this.headers,
      body: JSON.stringify(body),
    }});
    return response.json();
  }}
}}

export default AtomicPatentClient;
"#,
            package_name = config.package_name,
            version = config.package_version,
            base_url = config.base_url,
            api_version = config.api_version,
        );

        GeneratedSdk {
            language: "TypeScript".to_string(),
            code,
            package_name: config.package_name.clone(),
            version: config.package_version.clone(),
        }
    }

    /// Generate Python SDK
    pub fn generate_python(config: &SdkConfig) -> GeneratedSdk {
        let code = format!(
            r#"""
Atomic Patent API Client
Auto-generated from OpenAPI specification

@package {package_name}
@version {version}
"""

from typing import Optional, Dict, Any, List
from dataclasses import dataclass
import requests
import json

@dataclass
class ErrorDetails:
    code: str
    message: str
    details: Optional[Dict[str, List[str]]] = None

@dataclass
class ResponseMeta:
    request_id: str
    timestamp: int
    version: str

@dataclass
class ApiResponse:
    status: int
    message: str
    data: Optional[Any] = None
    error: Optional[ErrorDetails] = None
    meta: Optional[ResponseMeta] = None

@dataclass
class CommitIpRequest:
    owner: str
    commitment_hash: str

@dataclass
class IpRecord:
    ip_id: int
    owner: str
    commitment_hash: str
    timestamp: int
    revoked: bool

@dataclass
class InitiateSwapRequest:
    ip_registry_id: str
    ip_id: int
    seller: str
    price: int
    buyer: str
    token: str
    referrer: Optional[str] = None

@dataclass
class SwapRecord:
    ip_id: int
    ip_registry_id: str
    seller: str
    buyer: str
    price: int
    token: str
    status: str
    expiry: int

class AtomicPatentClient:
    """Atomic Patent API Client"""

    def __init__(self, base_url: str = "{base_url}", api_version: str = "{api_version}"):
        self.base_url = base_url
        self.api_version = api_version
        self.headers = {{
            "Content-Type": "application/json",
            "Accept": "application/json",
        }}

    def commit_ip(self, request: CommitIpRequest) -> ApiResponse:
        """Commit a new IP"""
        return self._post("/v1/ip/commit", request.__dict__)

    def get_ip(self, ip_id: int) -> ApiResponse:
        """Get an IP record by ID"""
        return self._get(f"/v1/ip/{{ip_id}}")

    def list_ip_by_owner(self, owner: str, limit: int = 50, offset: int = 0) -> ApiResponse:
        """List IPs by owner"""
        return self._get(f"/v1/ip/owner/{{owner}}?limit={{limit}}&offset={{offset}}")

    def initiate_swap(self, request: InitiateSwapRequest) -> ApiResponse:
        """Initiate a swap"""
        return self._post("/v1/swap/initiate", request.__dict__)

    def get_swap(self, swap_id: int) -> ApiResponse:
        """Get a swap record"""
        return self._get(f"/v1/swap/{{swap_id}}")

    def accept_swap(self, swap_id: int, buyer: str) -> ApiResponse:
        """Accept a swap"""
        return self._post(f"/v1/swap/{{swap_id}}/accept", {{"buyer": buyer}})

    def reveal_key(self, swap_id: int, caller: str, secret: str, blinding_factor: str) -> ApiResponse:
        """Reveal decryption key"""
        return self._post(f"/v1/swap/{{swap_id}}/reveal", {{
            "caller": caller,
            "secret": secret,
            "blinding_factor": blinding_factor,
        }})

    def cancel_swap(self, swap_id: int, canceller: str) -> ApiResponse:
        """Cancel a swap"""
        return self._post(f"/v1/swap/{{swap_id}}/cancel", {{"canceller": canceller}})

    def _get(self, path: str) -> ApiResponse:
        response = requests.get(f"{{self.base_url}}{{path}}", headers=self.headers)
        return ApiResponse(**response.json())

    def _post(self, path: str, body: Dict[str, Any]) -> ApiResponse:
        response = requests.post(
            f"{{self.base_url}}{{path}}",
            headers=self.headers,
            data=json.dumps(body),
        )
        return ApiResponse(**response.json())
"#,
            package_name = config.package_name,
            version = config.package_version,
            base_url = config.base_url,
            api_version = config.api_version,
        );

        GeneratedSdk {
            language: "Python".to_string(),
            code,
            package_name: config.package_name.clone(),
            version: config.package_version.clone(),
        }
    }

    /// Generate Go SDK
    pub fn generate_go(config: &SdkConfig) -> GeneratedSdk {
        let code = format!(
            r#"package {package_name}

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
)

// ApiResponse represents a standard API response
type ApiResponse struct {{
	Status  int         `json:"status"`
	Message string      `json:"message"`
	Data    interface{{}}`json:"data,omitempty"`
	Error   *ErrorDetails `json:"error,omitempty"`
	Meta    ResponseMeta  `json:"meta"`
}}

// ErrorDetails contains error information
type ErrorDetails struct {{
	Code    string              `json:"code"`
	Message string              `json:"message"`
	Details map[string][]string `json:"details,omitempty"`
}}

// ResponseMeta contains response metadata
type ResponseMeta struct {{
	RequestID string `json:"request_id"`
	Timestamp int64  `json:"timestamp"`
	Version   string `json:"version"`
}}

// CommitIpRequest represents a commit IP request
type CommitIpRequest struct {{
	Owner            string `json:"owner"`
	CommitmentHash   string `json:"commitment_hash"`
}}

// IpRecord represents an IP record
type IpRecord struct {{
	IpID             int64  `json:"ip_id"`
	Owner            string `json:"owner"`
	CommitmentHash   string `json:"commitment_hash"`
	Timestamp        int64  `json:"timestamp"`
	Revoked          bool   `json:"revoked"`
}}

// InitiateSwapRequest represents a swap initiation request
type InitiateSwapRequest struct {{
	IpRegistryID string `json:"ip_registry_id"`
	IpID         int64  `json:"ip_id"`
	Seller       string `json:"seller"`
	Price        int64  `json:"price"`
	Buyer        string `json:"buyer"`
	Token        string `json:"token"`
	Referrer     string `json:"referrer,omitempty"`
}}

// SwapRecord represents a swap record
type SwapRecord struct {{
	IpID         int64  `json:"ip_id"`
	IpRegistryID string `json:"ip_registry_id"`
	Seller       string `json:"seller"`
	Buyer        string `json:"buyer"`
	Price        int64  `json:"price"`
	Token        string `json:"token"`
	Status       string `json:"status"`
	Expiry       int64  `json:"expiry"`
}}

// Client represents the Atomic Patent API client
type Client struct {{
	BaseURL string
	Version string
	client  *http.Client
}}

// NewClient creates a new API client
func NewClient(baseURL string, version string) *Client {{
	return &Client{{
		BaseURL: baseURL,
		Version: version,
		client:  &http.Client{{}},
	}}
}}

// CommitIp commits a new IP
func (c *Client) CommitIp(req *CommitIpRequest) (*ApiResponse, error) {{
	return c.post("/v1/ip/commit", req)
}}

// GetIp retrieves an IP record
func (c *Client) GetIp(ipID int64) (*ApiResponse, error) {{
	return c.get(fmt.Sprintf("/v1/ip/%d", ipID))
}}

// ListIpByOwner lists IPs by owner
func (c *Client) ListIpByOwner(owner string, limit, offset int64) (*ApiResponse, error) {{
	return c.get(fmt.Sprintf("/v1/ip/owner/%s?limit=%d&offset=%d", owner, limit, offset))
}}

// InitiateSwap initiates a swap
func (c *Client) InitiateSwap(req *InitiateSwapRequest) (*ApiResponse, error) {{
	return c.post("/v1/swap/initiate", req)
}}

// GetSwap retrieves a swap record
func (c *Client) GetSwap(swapID int64) (*ApiResponse, error) {{
	return c.get(fmt.Sprintf("/v1/swap/%d", swapID))
}}

// AcceptSwap accepts a swap
func (c *Client) AcceptSwap(swapID int64, buyer string) (*ApiResponse, error) {{
	body := map[string]string{{"buyer": buyer}}
	return c.post(fmt.Sprintf("/v1/swap/%d/accept", swapID), body)
}}

// CancelSwap cancels a swap
func (c *Client) CancelSwap(swapID int64, canceller string) (*ApiResponse, error) {{
	body := map[string]string{{"canceller": canceller}}
	return c.post(fmt.Sprintf("/v1/swap/%d/cancel", swapID), body)
}}

func (c *Client) get(path string) (*ApiResponse, error) {{
	resp, err := c.client.Get(c.BaseURL + path)
	if err != nil {{
		return nil, err
	}}
	defer resp.Body.Close()
	return c.parseResponse(resp.Body)
}}

func (c *Client) post(path string, body interface{{}}) (*ApiResponse, error) {{
	data, err := json.Marshal(body)
	if err != nil {{
		return nil, err
	}}

	resp, err := c.client.Post(
		c.BaseURL+path,
		"application/json",
		bytes.NewBuffer(data),
	)
	if err != nil {{
		return nil, err
	}}
	defer resp.Body.Close()
	return c.parseResponse(resp.Body)
}}

func (c *Client) parseResponse(body io.Reader) (*ApiResponse, error) {{
	var resp ApiResponse
	if err := json.NewDecoder(body).Decode(&resp); err != nil {{
		return nil, err
	}}
	return &resp, nil
}}
"#,
            package_name = config.package_name.to_lowercase(),
        );

        GeneratedSdk {
            language: "Go".to_string(),
            code,
            package_name: config.package_name.clone(),
            version: config.package_version.clone(),
        }
    }

    /// Generate Rust SDK
    pub fn generate_rust(config: &SdkConfig) -> GeneratedSdk {
        let code = format!(
            r#"//! Atomic Patent API Client
//! Auto-generated from OpenAPI specification
//!
//! # Example
//!
//! ```no_run
//! use {package_name}::AtomicPatentClient;
//!
//! #[tokio::main]
//! async fn main() {{
//!     let client = AtomicPatentClient::new("{base_url}");
//!     // Use client...
//! }}
//! ```

use serde::{{Deserialize, Serialize}};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {{
    pub status: u16,
    pub message: String,
    pub data: Option<T>,
    pub error: Option<ErrorDetails>,
    pub meta: ResponseMeta,
}}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetails {{
    pub code: String,
    pub message: String,
    pub details: Option<HashMap<String, Vec<String>>>,
}}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMeta {{
    pub request_id: String,
    pub timestamp: u64,
    pub version: String,
}}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitIpRequest {{
    pub owner: String,
    pub commitment_hash: String,
}}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpRecord {{
    pub ip_id: u64,
    pub owner: String,
    pub commitment_hash: String,
    pub timestamp: u64,
    pub revoked: bool,
}}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitiateSwapRequest {{
    pub ip_registry_id: String,
    pub ip_id: u64,
    pub seller: String,
    pub price: i128,
    pub buyer: String,
    pub token: String,
    pub referrer: Option<String>,
}}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapRecord {{
    pub ip_id: u64,
    pub ip_registry_id: String,
    pub seller: String,
    pub buyer: String,
    pub price: i128,
    pub token: String,
    pub status: String,
    pub expiry: u64,
}}

pub struct AtomicPatentClient {{
    base_url: String,
    client: reqwest::Client,
}}

impl AtomicPatentClient {{
    /// Create a new client
    pub fn new(base_url: impl Into<String>) -> Self {{
        Self {{
            base_url: base_url.into(),
            client: reqwest::Client::new(),
        }}
    }}

    /// Commit a new IP
    pub async fn commit_ip(&self, req: CommitIpRequest) -> Result<ApiResponse<u64>, Box<dyn std::error::Error>> {{
        let url = format!("{{}}/v1/ip/commit", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        Ok(resp.json().await?)
    }}

    /// Get an IP record
    pub async fn get_ip(&self, ip_id: u64) -> Result<ApiResponse<IpRecord>, Box<dyn std::error::Error>> {{
        let url = format!("{{}}/v1/ip/{{}}", self.base_url, ip_id);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.json().await?)
    }}

    /// List IPs by owner
    pub async fn list_ip_by_owner(&self, owner: &str, limit: u64, offset: u64) -> Result<ApiResponse<Vec<u64>>, Box<dyn std::error::Error>> {{
        let url = format!("{{}}/v1/ip/owner/{{}}?limit={{}}&offset={{}}", self.base_url, owner, limit, offset);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.json().await?)
    }}

    /// Initiate a swap
    pub async fn initiate_swap(&self, req: InitiateSwapRequest) -> Result<ApiResponse<u64>, Box<dyn std::error::Error>> {{
        let url = format!("{{}}/v1/swap/initiate", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        Ok(resp.json().await?)
    }}

    /// Get a swap record
    pub async fn get_swap(&self, swap_id: u64) -> Result<ApiResponse<SwapRecord>, Box<dyn std::error::Error>> {{
        let url = format!("{{}}/v1/swap/{{}}", self.base_url, swap_id);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.json().await?)
    }}

    /// Accept a swap
    pub async fn accept_swap(&self, swap_id: u64, buyer: &str) -> Result<ApiResponse<()>, Box<dyn std::error::Error>> {{
        let url = format!("{{}}/v1/swap/{{}}/accept", self.base_url, swap_id);
        let body = serde_json::json!({{"buyer": buyer}});
        let resp = self.client.post(&url).json(&body).send().await?;
        Ok(resp.json().await?)
    }}

    /// Cancel a swap
    pub async fn cancel_swap(&self, swap_id: u64, canceller: &str) -> Result<ApiResponse<()>, Box<dyn std::error::Error>> {{
        let url = format!("{{}}/v1/swap/{{}}/cancel", self.base_url, swap_id);
        let body = serde_json::json!({{"canceller": canceller}});
        let resp = self.client.post(&url).json(&body).send().await?;
        Ok(resp.json().await?)
    }}
}}
"#,
            package_name = config.package_name.to_lowercase().replace("-", "_"),
            base_url = config.base_url,
        );

        GeneratedSdk {
            language: "Rust".to_string(),
            code,
            package_name: config.package_name.clone(),
            version: config.package_version.clone(),
        }
    }

    /// Generate SDKs for all supported languages
    pub fn generate_all(config: &SdkConfig) -> Vec<GeneratedSdk> {
        vec![
            Self::generate_typescript(config),
            Self::generate_python(config),
            Self::generate_go(config),
            Self::generate_rust(config),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_test_config() -> SdkConfig {
        SdkConfig {
            base_url: "https://api.atomicpatent.io".to_string(),
            api_version: "1.0.0".to_string(),
            package_name: "atomic-patent".to_string(),
            package_version: "1.0.0".to_string(),
        }
    }

    #[test]
    fn test_generate_typescript() {
        let config = get_test_config();
        let sdk = SdkGenerator::generate_typescript(&config);
        assert_eq!(sdk.language, "TypeScript");
        assert!(sdk.code.contains("AtomicPatentClient"));
        assert!(sdk.code.contains("commitIp"));
    }

    #[test]
    fn test_generate_python() {
        let config = get_test_config();
        let sdk = SdkGenerator::generate_python(&config);
        assert_eq!(sdk.language, "Python");
        assert!(sdk.code.contains("AtomicPatentClient"));
        assert!(sdk.code.contains("commit_ip"));
    }

    #[test]
    fn test_generate_go() {
        let config = get_test_config();
        let sdk = SdkGenerator::generate_go(&config);
        assert_eq!(sdk.language, "Go");
        assert!(sdk.code.contains("CommitIp"));
    }

    #[test]
    fn test_generate_rust() {
        let config = get_test_config();
        let sdk = SdkGenerator::generate_rust(&config);
        assert_eq!(sdk.language, "Rust");
        assert!(sdk.code.contains("commit_ip"));
    }

    #[test]
    fn test_generate_all() {
        let config = get_test_config();
        let sdks = SdkGenerator::generate_all(&config);
        assert_eq!(sdks.len(), 4);
        assert!(sdks.iter().any(|s| s.language == "TypeScript"));
        assert!(sdks.iter().any(|s| s.language == "Python"));
        assert!(sdks.iter().any(|s| s.language == "Go"));
        assert!(sdks.iter().any(|s| s.language == "Rust"));
    }
}
