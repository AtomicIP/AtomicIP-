"""
Reference implementation for AtomicIP contract logic (#375).

This module mirrors the core logic of the Rust/Soroban contracts in pure Python
so that differential tests can compare outputs and catch logic bugs.

Usage (differential tests call these functions directly):
    python3 -c "from reference_impl import *; print(commitment_hash(b'\\x01'*32, b'\\x02'*32).hex())"
"""

import hashlib
from dataclasses import dataclass, field
from typing import Optional


# ── Commitment scheme ─────────────────────────────────────────────────────────

def commitment_hash(secret: bytes, blinding_factor: bytes) -> bytes:
    """
    Compute the Pedersen-style commitment hash used by commit_ip.

    Mirrors: sha256(secret || blinding_factor)
    """
    assert len(secret) == 32, "secret must be 32 bytes"
    assert len(blinding_factor) == 32, "blinding_factor must be 32 bytes"
    return hashlib.sha256(secret + blinding_factor).digest()


def verify_commitment(stored_hash: bytes, secret: bytes, blinding_factor: bytes) -> bool:
    """
    Return True iff sha256(secret || blinding_factor) == stored_hash.

    Mirrors: IpRegistry::verify_commitment
    """
    return commitment_hash(secret, blinding_factor) == stored_hash


# ── IP Registry ───────────────────────────────────────────────────────────────

@dataclass
class IpRecord:
    ip_id: int
    owner: str
    commitment_hash: bytes
    timestamp: int
    revoked: bool = False


@dataclass
class IpRegistry:
    _records: dict = field(default_factory=dict)
    _owner_index: dict = field(default_factory=dict)
    _commitment_index: dict = field(default_factory=dict)
    _next_id: int = 1

    def commit_ip(self, owner: str, hash_bytes: bytes, timestamp: int = 0) -> int:
        """
        Register a new IP commitment. Returns the assigned IP ID.

        Raises ValueError for zero hash or duplicate hash.
        """
        if hash_bytes == bytes(32):
            raise ValueError("ZeroCommitmentHash")
        if hash_bytes in self._commitment_index:
            raise ValueError("CommitmentAlreadyRegistered")

        ip_id = self._next_id
        self._next_id += 1

        record = IpRecord(
            ip_id=ip_id,
            owner=owner,
            commitment_hash=hash_bytes,
            timestamp=timestamp,
        )
        self._records[ip_id] = record
        self._commitment_index[hash_bytes] = owner
        self._owner_index.setdefault(owner, []).append(ip_id)
        return ip_id

    def get_ip(self, ip_id: int) -> IpRecord:
        if ip_id not in self._records:
            raise KeyError(f"IpNotFound: {ip_id}")
        return self._records[ip_id]

    def revoke_ip(self, ip_id: int) -> None:
        record = self.get_ip(ip_id)
        if record.revoked:
            raise ValueError("IpAlreadyRevoked")
        record.revoked = True

    def list_ip_by_owner(self, owner: str) -> list:
        return list(self._owner_index.get(owner, []))

    def verify_commitment(self, ip_id: int, secret: bytes, blinding_factor: bytes) -> bool:
        record = self.get_ip(ip_id)
        return verify_commitment(record.commitment_hash, secret, blinding_factor)


# ── Atomic Swap ───────────────────────────────────────────────────────────────

from enum import Enum


class SwapStatus(Enum):
    Pending = "Pending"
    Accepted = "Accepted"
    Completed = "Completed"
    Cancelled = "Cancelled"
    Disputed = "Disputed"


@dataclass
class SwapRecord:
    swap_id: int
    ip_id: int
    seller: str
    buyer: str
    price: int
    status: SwapStatus = SwapStatus.Pending


@dataclass
class AtomicSwap:
    registry: IpRegistry
    _swaps: dict = field(default_factory=dict)
    _active_swaps: dict = field(default_factory=dict)  # ip_id → swap_id
    _next_id: int = 0

    def initiate_swap(self, ip_id: int, seller: str, price: int, buyer: str) -> int:
        """
        Seller initiates a patent sale. Returns swap ID.

        Raises ValueError for zero price, non-owner seller, or active swap.
        """
        if price <= 0:
            raise ValueError("PriceMustBeGreaterThanZero")

        record = self.registry.get_ip(ip_id)
        if record.owner != seller:
            raise ValueError("SellerIsNotTheIPOwner")
        if record.revoked:
            raise ValueError("IpIsRevoked")
        if ip_id in self._active_swaps:
            raise ValueError("ActiveSwapAlreadyExistsForThisIpId")

        swap_id = self._next_id
        self._next_id += 1

        swap = SwapRecord(
            swap_id=swap_id,
            ip_id=ip_id,
            seller=seller,
            buyer=buyer,
            price=price,
        )
        self._swaps[swap_id] = swap
        self._active_swaps[ip_id] = swap_id
        return swap_id

    def accept_swap(self, swap_id: int) -> None:
        swap = self._get_swap(swap_id)
        if swap.status != SwapStatus.Pending:
            raise ValueError("SwapNotPending")
        swap.status = SwapStatus.Accepted

    def reveal_key(self, swap_id: int, caller: str, secret: bytes, blinding_factor: bytes) -> None:
        swap = self._get_swap(swap_id)
        if swap.status != SwapStatus.Accepted:
            raise ValueError("SwapNotAccepted")
        if caller != swap.seller:
            raise ValueError("OnlyTheSellerCanRevealTheKey")
        if not self.registry.verify_commitment(swap.ip_id, secret, blinding_factor):
            raise ValueError("InvalidKey")
        swap.status = SwapStatus.Completed
        del self._active_swaps[swap.ip_id]

    def cancel_swap(self, swap_id: int, caller: str) -> None:
        swap = self._get_swap(swap_id)
        if caller not in (swap.seller, swap.buyer):
            raise ValueError("OnlyTheSellerOrBuyerCanCancel")
        if swap.status not in (SwapStatus.Pending, SwapStatus.Accepted):
            raise ValueError("CannotCancelInCurrentState")
        swap.status = SwapStatus.Cancelled
        self._active_swaps.pop(swap.ip_id, None)

    def get_swap(self, swap_id: int) -> SwapRecord:
        return self._get_swap(swap_id)

    def _get_swap(self, swap_id: int) -> SwapRecord:
        if swap_id not in self._swaps:
            raise KeyError(f"SwapNotFound: {swap_id}")
        return self._swaps[swap_id]
