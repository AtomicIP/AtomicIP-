#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, BytesN, Env};

// ── Storage Keys ─────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Swap(u64),
    NextId,
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub enum SwapStatus {
    Pending,
    Accepted,
    Completed,
    Cancelled,
}

#[contracttype]
#[derive(Clone)]
pub struct SwapRecord {
    pub ip_id: u64,
    pub seller: Address,
    pub buyer: Address,
    pub price: i128,
    pub token: Address,
    pub status: SwapStatus,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct AtomicSwap;

#[contractimpl]
impl AtomicSwap {
    /// Seller initiates a patent sale. Returns the swap ID.
    pub fn initiate_swap(
        env: Env,
        seller: Address,
        ip_id: u64,
        price: i128,
        token: Address,
        buyer: Address,
    ) -> u64 {
        seller.require_auth();
        let id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(0);

        let swap = SwapRecord {
            ip_id,
            seller,
            buyer,
            price,
            token,
            status: SwapStatus::Pending,
        };

        env.storage().persistent().set(&DataKey::Swap(id), &swap);
        env.storage().persistent().set(&DataKey::NextId, &(id + 1));
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::NextId, TTL_THRESHOLD, TTL_BUMP);
        id
    }

    /// Buyer accepts the swap and transfers payment into contract escrow.
    pub fn accept_swap(env: Env, swap_id: u64) {
        let mut swap: SwapRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Swap(swap_id))
            .expect("swap not found");

        swap.buyer.require_auth();
        assert!(swap.status == SwapStatus::Pending, "swap not pending");

        swap.buyer.require_auth();

        // Transfer buyer's payment into contract escrow
        let token_client = token::Client::new(&env, &swap.token);
        token_client.transfer(&swap.buyer, &env.current_contract_address(), &swap.price);

        swap.status = SwapStatus::Accepted;
        env.storage().persistent().set(&DataKey::Swap(swap_id), &swap);
    }

    /// Seller reveals the decryption key; escrowed payment releases to seller.
    pub fn reveal_key(env: Env, swap_id: u64, _decryption_key: BytesN<32>) {
        let mut swap: SwapRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Swap(swap_id))
            .expect("swap not found");

        swap.seller.require_auth();
        assert!(swap.status == SwapStatus::Accepted, "swap not accepted");

        swap.seller.require_auth();

        // Release escrowed payment to seller
        let token_client = token::Client::new(&env, &swap.token);
        token_client.transfer(&env.current_contract_address(), &swap.seller, &swap.price);

        swap.status = SwapStatus::Completed;
        env.storage().persistent().set(&DataKey::Swap(swap_id), &swap);
    }

    /// Cancel a swap — refunds buyer if payment was escrowed.
    pub fn cancel_swap(env: Env, swap_id: u64) {
        let mut swap: SwapRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Swap(swap_id))
            .expect("swap not found");

        assert!(
            swap.status == SwapStatus::Pending || swap.status == SwapStatus::Accepted,
            "swap already finalised"
        );

        // Refund buyer if payment is already in escrow
        if swap.status == SwapStatus::Accepted {
            let token_client = token::Client::new(&env, &swap.token);
            token_client.transfer(&env.current_contract_address(), &swap.buyer, &swap.price);
        }

        swap.status = SwapStatus::Cancelled;
        env.storage().persistent().set(&DataKey::Swap(swap_id), &swap);
    }

    /// Read a swap record.
    pub fn get_swap(env: Env, swap_id: u64) -> SwapRecord {
        env.storage()
            .persistent()
            .get(&DataKey::Swap(swap_id))
            .expect("swap not found")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation},
        token::{Client as TokenClient, StellarAssetClient},
        Address, BytesN, Env, IntoVal,
    };

    fn setup_token(env: &Env, admin: &Address) -> Address {
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        StellarAssetClient::new(env, &token_id.address()).mint(admin, &10_000);
        token_id.address()
    }

    #[test]
    fn test_escrow_balance_on_accept() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token_addr = setup_token(&env, &buyer);
        let token = TokenClient::new(&env, &token_addr);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&seller, &1u64, &500i128, &token_addr, &buyer);

        assert_eq!(token.balance(&buyer), 10_000);
        assert_eq!(token.balance(&contract_id), 0);

        client.accept_swap(&swap_id);

        // Payment must be in escrow
        assert_eq!(token.balance(&buyer), 9_500);
        assert_eq!(token.balance(&contract_id), 500);
    }

    #[test]
    fn test_reveal_key_releases_payment_to_seller() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token_addr = setup_token(&env, &buyer);
        let token = TokenClient::new(&env, &token_addr);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&seller, &1u64, &500i128, &token_addr, &buyer);
        client.accept_swap(&swap_id);

        let key = BytesN::from_array(&env, &[0u8; 32]);
        client.reveal_key(&swap_id, &key);

        assert_eq!(token.balance(&seller), 500);
        assert_eq!(token.balance(&contract_id), 0);
    }

    #[test]
    fn test_cancel_after_accept_refunds_buyer() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token_addr = setup_token(&env, &buyer);
        let token = TokenClient::new(&env, &token_addr);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&seller, &1u64, &500i128, &token_addr, &buyer);
        client.accept_swap(&swap_id);

        assert_eq!(token.balance(&contract_id), 500);

        client.cancel_swap(&swap_id);

        assert_eq!(token.balance(&buyer), 10_000); // fully refunded
        assert_eq!(token.balance(&contract_id), 0);
    }
}
