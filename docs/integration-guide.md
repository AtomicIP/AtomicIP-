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

## Support

- GitHub Issues: https://github.com/AtomicIP/AtomicIP-/issues
- Documentation: https://github.com/AtomicIP/AtomicIP-/tree/main/docs
