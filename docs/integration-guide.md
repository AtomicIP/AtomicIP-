# Integration Guide for Wallet Providers

This guide helps wallet providers integrate Atomic Patent IP registry and atomic swap functionality.

## Contract Interface

### IP Registry Contract

**commit_ip** — Register a new IP commitment
```rust
fn commit_ip(owner: Address, commitment_hash: BytesN<32>) -> u64
```
- `owner`: Address that owns the IP (requires auth)
- `commitment_hash`: SHA-256 hash of `secret || blinding_factor`
- Returns: Unique IP ID

**get_ip** — Retrieve IP record
```rust
fn get_ip(ip_id: u64) -> IpRecord
```
Returns:
```rust
struct IpRecord {
    ip_id: u64,
    owner: Address,
    commitment_hash: BytesN<32>,
    timestamp: u64,
    revoked: bool,
}
```

**list_ip_by_owner** — List all IPs owned by an address
```rust
fn list_ip_by_owner(owner: Address) -> Vec<u64>
```

**transfer_ip** — Transfer IP ownership
```rust
fn transfer_ip(ip_id: u64, new_owner: Address)
```

**revoke_ip** — Mark IP as revoked
```rust
fn revoke_ip(ip_id: u64)
```

### Atomic Swap Contract

**initiate_swap** — Seller initiates a patent sale
```rust
fn initiate_swap(
    token: Address,
    ip_id: u64,
    seller: Address,
    price: i128,
    buyer: Address
) -> u64
```
- `token`: Token contract address (e.g., XLM, USDC)
- Returns: Swap ID

**accept_swap** — Buyer accepts and sends payment to escrow
```rust
fn accept_swap(swap_id: u64)
```

**reveal_key** — Seller reveals decryption key
```rust
fn reveal_key(
    swap_id: u64,
    caller: Address,
    secret: BytesN<32>,
    blinding_factor: BytesN<32>
)
```

**cancel_swap** — Cancel pending swap
```rust
fn cancel_swap(swap_id: u64, canceller: Address)
```

**cancel_expired_swap** — Buyer cancels expired accepted swap
```rust
fn cancel_expired_swap(swap_id: u64, caller: Address)
```

**get_swap** — Retrieve swap details
```rust
fn get_swap(swap_id: u64) -> Option<SwapRecord>
```

## Integration Examples

### TypeScript/JavaScript (Stellar SDK)

```typescript
import { Contract, SorobanRpc, TransactionBuilder, Networks } from '@stellar/stellar-sdk';

const rpcUrl = 'https://soroban-testnet.stellar.org';
const server = new SorobanRpc.Server(rpcUrl);

// Commit IP
async function commitIP(
  registryAddress: string,
  ownerKeypair: Keypair,
  commitmentHash: Buffer
): Promise<string> {
  const contract = new Contract(registryAddress);
  const account = await server.getAccount(ownerKeypair.publicKey());
  
  const tx = new TransactionBuilder(account, {
    fee: '1000',
    networkPassphrase: Networks.TESTNET
  })
    .addOperation(
      contract.call(
        'commit_ip',
        xdr.ScVal.scvAddress(ownerKeypair.publicKey()),
        xdr.ScVal.scvBytes(commitmentHash)
      )
    )
    .setTimeout(30)
    .build();
  
  tx.sign(ownerKeypair);
  const result = await server.sendTransaction(tx);
  return result.hash;
}

// Initiate swap
async function initiateSwap(
  swapAddress: string,
  tokenAddress: string,
  ipId: bigint,
  sellerKeypair: Keypair,
  price: bigint,
  buyerAddress: string
): Promise<bigint> {
  const contract = new Contract(swapAddress);
  const account = await server.getAccount(sellerKeypair.publicKey());
  
  const tx = new TransactionBuilder(account, {
    fee: '1000',
    networkPassphrase: Networks.TESTNET
  })
    .addOperation(
      contract.call(
        'initiate_swap',
        xdr.ScVal.scvAddress(tokenAddress),
        xdr.ScVal.scvU64(ipId),
        xdr.ScVal.scvAddress(sellerKeypair.publicKey()),
        xdr.ScVal.scvI128(price),
        xdr.ScVal.scvAddress(buyerAddress)
      )
    )
    .setTimeout(30)
    .build();
  
  tx.sign(sellerKeypair);
  const result = await server.sendTransaction(tx);
  // Parse swap_id from result
  return parseSwapId(result);
}
```

### Python (stellar-sdk)

```python
from stellar_sdk import Soroban, Keypair, Network, TransactionBuilder
from stellar_sdk.soroban_rpc import SorobanServer

rpc_url = "https://soroban-testnet.stellar.org"
server = SorobanServer(rpc_url)

def commit_ip(registry_address: str, owner_keypair: Keypair, commitment_hash: bytes) -> str:
    contract = Soroban.Contract(registry_address)
    source = server.load_account(owner_keypair.public_key)
    
    tx = (
        TransactionBuilder(source, Network.TESTNET_NETWORK_PASSPHRASE, base_fee=1000)
        .append_invoke_contract_function_op(
            contract_id=registry_address,
            function_name="commit_ip",
            parameters=[
                Soroban.to_address(owner_keypair.public_key),
                Soroban.to_bytes(commitment_hash)
            ]
        )
        .set_timeout(30)
        .build()
    )
    
    tx.sign(owner_keypair)
    response = server.send_transaction(tx)
    return response.hash

def accept_swap(swap_address: str, buyer_keypair: Keypair, swap_id: int) -> str:
    contract = Soroban.Contract(swap_address)
    source = server.load_account(buyer_keypair.public_key)
    
    tx = (
        TransactionBuilder(source, Network.TESTNET_NETWORK_PASSPHRASE, base_fee=1000)
        .append_invoke_contract_function_op(
            contract_id=swap_address,
            function_name="accept_swap",
            parameters=[Soroban.to_uint64(swap_id)]
        )
        .set_timeout(30)
        .build()
    )
    
    tx.sign(buyer_keypair)
    response = server.send_transaction(tx)
    return response.hash
```

## Wallet UI Recommendations

### IP Registration Flow
1. User enters IP description/document
2. Wallet generates `secret = sha256(document)`
3. Wallet generates random `blinding_factor`
4. Wallet computes `commitment_hash = sha256(secret || blinding_factor)`
5. Wallet stores `secret` and `blinding_factor` securely (encrypted local storage)
6. Wallet calls `commit_ip(user_address, commitment_hash)`
7. Display IP ID and timestamp to user

### Swap Initiation Flow (Seller)
1. User selects IP from their portfolio
2. User enters price and buyer address
3. Wallet calls `initiate_swap(token, ip_id, seller, price, buyer)`
4. Display swap ID and status

### Swap Acceptance Flow (Buyer)
1. User views pending swap details
2. Wallet shows price and IP metadata
3. User confirms payment
4. Wallet calls `accept_swap(swap_id)` (transfers payment to escrow)
5. Display "Waiting for seller to reveal key"

### Key Reveal Flow (Seller)
1. Seller views accepted swap
2. Wallet retrieves stored `secret` and `blinding_factor`
3. Wallet calls `reveal_key(swap_id, seller, secret, blinding_factor)`
4. Payment released to seller
5. Display "Swap completed"

## Security Considerations

- **Never expose secrets**: Store `secret` and `blinding_factor` encrypted
- **Verify commitment hashes**: Before revealing, confirm the hash matches
- **Handle expiry**: Notify buyers when swaps are near expiry
- **Token allowances**: Ensure buyer has approved token transfer before `accept_swap`
- **Gas estimation**: Pre-simulate transactions to estimate fees

## Testnet Deployment

- Network: `testnet`
- RPC URL: `https://soroban-testnet.stellar.org`
- Contract addresses: See [README deployment status](#)

## Circuit Breaker Configuration

The API server wraps every external service call (Soroban RPC, databases, price oracles) in a
circuit breaker that prevents cascading failures when a dependency becomes unavailable.

### State Machine

```
           ┌─────────────────┐
           │     CLOSED      │◄──────────────────────────────────────────┐
           │  (normal flow)  │                                           │
           └────────┬────────┘                                           │
                    │ failure_threshold consecutive failures             │
                    ▼                                                     │
           ┌─────────────────┐   timeout_secs elapses   ┌───────────────┴──┐
           │      OPEN       │─────────────────────────►│    HALF-OPEN     │
           │ (rejects calls) │                           │  (test requests) │
           └─────────────────┘◄──────────────────────── └──────────────────┘
                                  any failure               success_threshold
                                                          consecutive successes
```

| Transition | Trigger |
|---|---|
| Closed → Open | `failure_threshold` consecutive failures |
| Open → HalfOpen | `timeout_secs` seconds elapse since last failure |
| HalfOpen → Closed | `success_threshold` consecutive successes (recovery) |
| HalfOpen → Open | Any failure during test requests |

### Configuration Parameters

| Parameter | Default | Description |
|---|---|---|
| `failure_threshold` | `5` | Consecutive failures before opening the circuit |
| `success_threshold` | `2` | Consecutive successes in HalfOpen state before closing |
| `timeout_secs` | `30` | Seconds the circuit stays Open before allowing test requests |
| `half_open_max_calls` | `3` | Maximum concurrent test requests allowed in HalfOpen state |

### Emitted Metrics

All metrics are exported in Prometheus format via `GET /metrics`.

| Metric | Type | Labels | Description |
|---|---|---|---|
| `circuit_breaker_state_transitions_total` | Counter | `service`, `from`, `to` | Incremented on every state change |
| `circuit_breaker_state` | Gauge | `service` | Current state: `0`=closed, `1`=open, `2`=half_open |
| `circuit_breaker_calls_total` | Counter | `service` | Total calls attempted through the breaker |
| `circuit_breaker_calls_rejected_total` | Counter | `service`, `state` | Calls rejected because the circuit is open |

### Named Circuit Breakers

Each external service gets its own named circuit breaker so failures are isolated:

```rust
use api_server::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};

let oracle_cb = CircuitBreaker::new(
    "price-oracle",
    CircuitBreakerConfig {
        failure_threshold: 5,
        success_threshold: 2,
        timeout_secs: 30,
        half_open_max_calls: 3,
    },
);

let db_cb = CircuitBreaker::new(
    "postgres",
    CircuitBreakerConfig::default(), // same defaults
);
```

### Using the `call` Helper

The `call` method wraps a closure and automatically records success or failure:

```rust
use api_server::circuit_breaker::CallError;

let result = oracle_cb.call(|| fetch_price_from_oracle());

match result {
    Ok(price) => { /* use price */ }
    Err(CallError::CircuitOpen) => {
        // Oracle circuit is open — return cached value or degrade gracefully
    }
    Err(CallError::ServiceError(e)) => {
        // Oracle responded but returned an error
    }
}
```

### Alerting Recommendations

Configure alerts on these Prometheus expressions:

```promql
# Circuit has been open for more than 60 seconds
circuit_breaker_state{service="price-oracle"} == 1

# Rejection rate exceeds 10% of all attempts
rate(circuit_breaker_calls_rejected_total[1m])
  / rate(circuit_breaker_calls_total[1m]) > 0.1

# More than 3 state transitions per minute (thrashing)
rate(circuit_breaker_state_transitions_total[1m]) > 3
```

## Support

- GitHub Issues: https://github.com/AtomicIP/AtomicIP-/issues
- Documentation: https://github.com/AtomicIP/AtomicIP-/tree/main/docs

---

## Observability — Distributed Tracing Setup

Atomic Patent API ships with OpenTelemetry (OTel) instrumentation that exports
spans via the OTLP protocol. Any OTLP-compatible backend works out of the box:
Jaeger, Datadog Agent, Grafana Tempo, Honeycomb, OpenTelemetry Collector, etc.

### Environment Variables

| Variable | Default | Description |
|---|---|---|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | `http://localhost:4317` | gRPC OTLP endpoint of your backend or Collector |
| `OTEL_SERVICE_NAME` | `atomic-patent-api` | Service name that appears in trace UIs |
| `OTEL_ENABLED` | `true` | Set to `false` to disable OTel and use plain JSON logs only |
| `RUST_LOG` | `info` | Log level for the `tracing` subscriber (e.g. `debug`, `info,api_server=debug`) |

### Quick Start — Jaeger (all-in-one)

```bash
# 1. Start Jaeger with OTLP gRPC enabled
docker run -d --name jaeger \
  -p 16686:16686 \
  -p 4317:4317 \
  jaegertracing/all-in-one:latest \
  --collector.otlp.enabled=true

# 2. Run the API server — it will auto-discover the local Jaeger instance
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 \
OTEL_SERVICE_NAME=atomic-patent-api \
cargo run -p api-server

# 3. Open the Jaeger UI
open http://localhost:16686
```

### Quick Start — Datadog Agent

```bash
# 1. Start the Datadog Agent with OTLP intake enabled
docker run -d --name dd-agent \
  -e DD_API_KEY=<YOUR_DD_API_KEY> \
  -e DD_OTLP_CONFIG_RECEIVER_PROTOCOLS_GRPC_ENDPOINT=0.0.0.0:4317 \
  -p 4317:4317 \
  gcr.io/datadoghq/agent:latest

# 2. Run the API server
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 \
OTEL_SERVICE_NAME=atomic-patent-api \
cargo run -p api-server
```

### Quick Start — OpenTelemetry Collector (recommended for production)

```yaml
# otel-collector-config.yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317

exporters:
  otlp/jaeger:
    endpoint: jaeger:4317
    tls:
      insecure: true
  datadog:
    api:
      key: ${DD_API_KEY}

service:
  pipelines:
    traces:
      receivers: [otlp]
      exporters: [otlp/jaeger, datadog]   # fan-out to multiple backends
```

```bash
docker run --rm -v $(pwd)/otel-collector-config.yaml:/etc/otel/config.yaml \
  -p 4317:4317 \
  otel/opentelemetry-collector-contrib:latest \
  --config /etc/otel/config.yaml
```

### Span Hierarchy

Every HTTP request creates a root server span. Contract operations appear as
child spans with domain-specific attributes:

```
HTTP POST /ip/commit               ← root span (HTTP semantic conventions)
└── ip.commit_ip                   ← child span
      ip.owner      = "GABCD..."
      ip.commitment_hash = "deadbeef..."

HTTP POST /swap/initiate
└── swap.initiate
      swap.ip_id  = 42
      swap.seller = "GABCD..."
      swap.buyer  = "GXYZ..."

HTTP POST /batch
└── batch.commit
      batch.size = 10

└── batch.escrow
      batch.ip_count = 5
```

### Correlation ID Headers

The API propagates trace correlation via standard headers. Include these in
outbound calls and inspect them in responses for end-to-end tracing.

| Header | Direction | Description |
|---|---|---|
| `traceparent` | request + response | W3C Trace Context (trace-id + span-id). Used by OTel SDK for distributed context propagation. |
| `X-Trace-ID` | request + response | Human-readable trace UUID. Use this to search in Jaeger / Datadog. |
| `X-Span-ID` | request + response | Current span UUID. Pass as `X-Span-ID` on the *next* outbound call to link parent → child spans. |

**Example — propagating trace context between services:**

```typescript
// Service A calls Service B, forwarding the trace context.
const response = await fetch('https://api.atomicpatent.io/ip/commit', {
  method: 'POST',
  headers: {
    'Content-Type': 'application/json',
    // Forward context from the upstream request received by Service A:
    'traceparent': upstreamRequest.headers.get('traceparent') ?? '',
    'X-Trace-ID':  upstreamRequest.headers.get('X-Trace-ID') ?? '',
    // Service A's current span becomes the parent for Service B's span:
    'X-Span-ID':   serviceACurrentSpanId,
  },
  body: JSON.stringify({ owner, commitment_hash }),
});

// The trace_id in Service B's response will match Service A's trace_id.
const traceId = response.headers.get('X-Trace-ID');
```

### Disabling Tracing

```bash
OTEL_ENABLED=false cargo run -p api-server
```

When disabled, the server falls back to structured JSON logs via
`tracing-subscriber` without any OTLP export. All `tracing::info!/warn!/error!`
calls still work normally.

---

## Blue-Green Deployment Guide

Atomic Patent API supports zero-downtime deployments using a blue-green deployment
strategy. Two identical environments (blue and green) are maintained, with traffic
routed to the active slot. Deployments target the inactive slot, then switch traffic
after validation.

### Architecture

```
                        ┌──────────────┐
                        │   Load       │
                        │   Balancer   │
                        └──────┬───────┘
                               │
                ┌──────────────┼──────────────┐
                ▼              ▼              ▼
        ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
        │   Active     │ │   Inactive   │ │   Previous   │
        │   Slot      │ │   Slot      │ │   Slot      │
        └──────────────┘ └──────────────┘ └──────────────┘
```

### Deployment Script

```bash
# Deploy version v1.2.3 to production
./scripts/deploy-bluegreen.sh \
    --env production \
    --version v1.2.3 \
    --health-url https://api.atomicip.io/health \
    --smoke-test \
    --rollback \
    --timeout 120
```

### Options

| Option | Default | Description |
|---|---|---|
| `--env NAME` | `production` | Target environment |
| `--version TAG` | (required) | Docker image tag or version identifier |
| `--health-url URL` | `http://localhost:8080/health` | Health check endpoint |
| `--smoke-test` | off | Run smoke tests after traffic switch |
| `--rollback` | off | Auto-rollback on smoke test failure or error spike |
| `--timeout SECS` | `120` | Health check timeout |
| `--verbose` | off | Verbose output |

### Deployment Flow

1. **Provision** — Create or verify the inactive slot environment
2. **Deploy** — Deploy the new version to the inactive slot
3. **Health Check** — Wait for health endpoint to return `healthy`
4. **Traffic Switch** — Route traffic to the new slot
5. **Smoke Tests** — Validate critical endpoints (optional)
6. **Monitor** — Observe error rates for 60 seconds post-deployment
7. **Rollback** — Auto-revert if error rate exceeds 5% threshold (optional)

### Health Check Endpoints

| Endpoint | Purpose |
|---|---|
| `GET /health` | Overall health (healthy/degraded) |
| `GET /health/detailed` | Per-component health with latency |
| `GET /metrics` | Prometheus metrics for monitoring |

### Health Check Script

```bash
# Quick health check
./scripts/health-check.sh http://localhost:8080

# Detailed component-level check
./scripts/health-check.sh http://localhost:8080 --detailed
```

### Smoke Test Suite

Post-deployment smoke tests validate:
- Health endpoint returns 200
- Metrics endpoint is accessible
- Version endpoint returns version info
- API responds to requests (expected 404s are acceptable)
- WebSocket upgrade handshake succeeds

```bash
# Run smoke tests manually
./scripts/smoke-test.sh
```

### Automatic Rollback

Rollback triggers activate when:
- **Health check timeout** — New slot fails to become healthy within timeout
- **Smoke test failure** — Post-switch validation fails
- **Error rate spike** — HTTP 5xx rate exceeds 5% threshold during monitoring window

Rollback reverts traffic to the previous slot using symlink atomically:

```bash
# The rollback mechanism:
# 1. Detects failure (health/smoke/error-rate)
# 2. Reads PREVIOUS_SYMLINK to find last known good slot
# 3. Atomically switches ACTIVE_SYMLINK back
# 4. Re-runs health check on rolled-back slot

# Manual rollback if needed:
./scripts/rollback.sh --url http://localhost:8080 --threshold 0.05 --duration 60
```

### Rollback Monitor Script

```bash
# Monitor error rate and auto-rollback on threshold breach
./scripts/rollback.sh \
    --url https://api.atomicip.io \
    --threshold 0.05 \
    --duration 120
```

### Zero-Downtime Guarantee

The deployment ensures zero data loss through:
- In-flight requests complete on the old slot before traffic switch
- New requests route to the new slot after switch
- Rollback restores previous slot within seconds
- Database schema changes are backward-compatible for one release cycle
