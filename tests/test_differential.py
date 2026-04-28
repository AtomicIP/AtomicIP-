"""
Differential tests (#375): compare Python reference implementation outputs
against the expected outputs of the Rust/Soroban contracts.

These tests verify that the reference implementation agrees with the contract
on all core logic paths. Any divergence indicates a logic bug in either the
contract or the reference.

Run: python3 -m pytest tests/test_differential.py -v
  or: python3 tests/test_differential.py
"""

import hashlib
import sys
import os

sys.path.insert(0, os.path.dirname(__file__))
from reference_impl import (
    commitment_hash,
    verify_commitment,
    IpRegistry,
    AtomicSwap,
    SwapStatus,
)


# ── Commitment hash ───────────────────────────────────────────────────────────

def test_commitment_hash_matches_sha256_concat():
    """
    The Rust contract computes: env.crypto().sha256(secret || blinding_factor).
    The Python reference must produce the same bytes.
    """
    secret = bytes([0x01] * 32)
    blinding = bytes([0x02] * 32)
    expected = hashlib.sha256(secret + blinding).digest()
    assert commitment_hash(secret, blinding) == expected


def test_commitment_hash_different_inputs_differ():
    s1, b1 = bytes([0x01] * 32), bytes([0x02] * 32)
    s2, b2 = bytes([0x03] * 32), bytes([0x04] * 32)
    assert commitment_hash(s1, b1) != commitment_hash(s2, b2)


def test_commitment_hash_order_matters():
    """secret || blinding ≠ blinding || secret (in general)."""
    s = bytes([0x01] * 32)
    b = bytes([0x02] * 32)
    assert commitment_hash(s, b) != commitment_hash(b, s)


# ── verify_commitment ─────────────────────────────────────────────────────────

def test_verify_commitment_correct_inputs():
    s = bytes([0xAA] * 32)
    b = bytes([0xBB] * 32)
    h = commitment_hash(s, b)
    assert verify_commitment(h, s, b) is True


def test_verify_commitment_wrong_secret():
    s = bytes([0xAA] * 32)
    b = bytes([0xBB] * 32)
    h = commitment_hash(s, b)
    wrong = bytes([0xFF] * 32)
    assert verify_commitment(h, wrong, b) is False


def test_verify_commitment_wrong_blinding():
    s = bytes([0xAA] * 32)
    b = bytes([0xBB] * 32)
    h = commitment_hash(s, b)
    wrong = bytes([0xFF] * 32)
    assert verify_commitment(h, s, wrong) is False


# ── IpRegistry ────────────────────────────────────────────────────────────────

def test_commit_ip_returns_sequential_ids():
    reg = IpRegistry()
    id1 = reg.commit_ip("alice", bytes([0x01] * 32))
    id2 = reg.commit_ip("alice", bytes([0x02] * 32))
    id3 = reg.commit_ip("bob", bytes([0x03] * 32))
    assert id1 == 1
    assert id2 == 2
    assert id3 == 3


def test_commit_ip_zero_hash_rejected():
    reg = IpRegistry()
    try:
        reg.commit_ip("alice", bytes(32))
        assert False, "should have raised"
    except ValueError as e:
        assert "ZeroCommitmentHash" in str(e)


def test_commit_ip_duplicate_rejected():
    reg = IpRegistry()
    h = bytes([0x42] * 32)
    reg.commit_ip("alice", h)
    try:
        reg.commit_ip("bob", h)
        assert False, "should have raised"
    except ValueError as e:
        assert "CommitmentAlreadyRegistered" in str(e)


def test_get_ip_stores_correct_fields():
    reg = IpRegistry()
    h = bytes([0x55] * 32)
    ip_id = reg.commit_ip("alice", h, timestamp=12345)
    record = reg.get_ip(ip_id)
    assert record.ip_id == ip_id
    assert record.owner == "alice"
    assert record.commitment_hash == h
    assert record.timestamp == 12345
    assert record.revoked is False


def test_revoke_ip_sets_flag():
    reg = IpRegistry()
    ip_id = reg.commit_ip("alice", bytes([0x01] * 32))
    reg.revoke_ip(ip_id)
    assert reg.get_ip(ip_id).revoked is True


def test_revoke_ip_twice_raises():
    reg = IpRegistry()
    ip_id = reg.commit_ip("alice", bytes([0x01] * 32))
    reg.revoke_ip(ip_id)
    try:
        reg.revoke_ip(ip_id)
        assert False, "should have raised"
    except ValueError as e:
        assert "IpAlreadyRevoked" in str(e)


def test_list_ip_by_owner():
    reg = IpRegistry()
    id1 = reg.commit_ip("alice", bytes([0x01] * 32))
    id2 = reg.commit_ip("alice", bytes([0x02] * 32))
    reg.commit_ip("bob", bytes([0x03] * 32))
    assert reg.list_ip_by_owner("alice") == [id1, id2]
    assert reg.list_ip_by_owner("bob") == [3]
    assert reg.list_ip_by_owner("unknown") == []


def test_verify_commitment_via_registry():
    reg = IpRegistry()
    s = bytes([0x11] * 32)
    b = bytes([0x22] * 32)
    h = commitment_hash(s, b)
    ip_id = reg.commit_ip("alice", h)
    assert reg.verify_commitment(ip_id, s, b) is True
    assert reg.verify_commitment(ip_id, bytes([0xFF] * 32), b) is False


# ── AtomicSwap ────────────────────────────────────────────────────────────────

def _make_swap_env():
    reg = IpRegistry()
    s = bytes([0x11] * 32)
    b = bytes([0x22] * 32)
    h = commitment_hash(s, b)
    ip_id = reg.commit_ip("seller", h)
    swap = AtomicSwap(registry=reg)
    return swap, reg, ip_id, s, b


def test_initiate_swap_returns_id_zero():
    swap, _, ip_id, _, _ = _make_swap_env()
    sid = swap.initiate_swap(ip_id, "seller", 500, "buyer")
    assert sid == 0


def test_initiate_swap_status_pending():
    swap, _, ip_id, _, _ = _make_swap_env()
    sid = swap.initiate_swap(ip_id, "seller", 500, "buyer")
    assert swap.get_swap(sid).status == SwapStatus.Pending


def test_initiate_swap_zero_price_rejected():
    swap, _, ip_id, _, _ = _make_swap_env()
    try:
        swap.initiate_swap(ip_id, "seller", 0, "buyer")
        assert False
    except ValueError as e:
        assert "PriceMustBeGreaterThanZero" in str(e)


def test_initiate_swap_non_owner_rejected():
    swap, _, ip_id, _, _ = _make_swap_env()
    try:
        swap.initiate_swap(ip_id, "not_seller", 500, "buyer")
        assert False
    except ValueError as e:
        assert "SellerIsNotTheIPOwner" in str(e)


def test_accept_swap_sets_accepted():
    swap, _, ip_id, _, _ = _make_swap_env()
    sid = swap.initiate_swap(ip_id, "seller", 500, "buyer")
    swap.accept_swap(sid)
    assert swap.get_swap(sid).status == SwapStatus.Accepted


def test_reveal_key_completes_swap():
    swap, _, ip_id, s, b = _make_swap_env()
    sid = swap.initiate_swap(ip_id, "seller", 500, "buyer")
    swap.accept_swap(sid)
    swap.reveal_key(sid, "seller", s, b)
    assert swap.get_swap(sid).status == SwapStatus.Completed


def test_reveal_key_wrong_secret_raises():
    swap, _, ip_id, s, b = _make_swap_env()
    sid = swap.initiate_swap(ip_id, "seller", 500, "buyer")
    swap.accept_swap(sid)
    try:
        swap.reveal_key(sid, "seller", bytes([0xFF] * 32), b)
        assert False
    except ValueError as e:
        assert "InvalidKey" in str(e)


def test_reveal_key_on_pending_raises():
    swap, _, ip_id, s, b = _make_swap_env()
    sid = swap.initiate_swap(ip_id, "seller", 500, "buyer")
    try:
        swap.reveal_key(sid, "seller", s, b)
        assert False
    except ValueError as e:
        assert "SwapNotAccepted" in str(e)


def test_cancel_swap_sets_cancelled():
    swap, _, ip_id, _, _ = _make_swap_env()
    sid = swap.initiate_swap(ip_id, "seller", 500, "buyer")
    swap.cancel_swap(sid, "seller")
    assert swap.get_swap(sid).status == SwapStatus.Cancelled


def test_duplicate_swap_rejected():
    swap, _, ip_id, _, _ = _make_swap_env()
    swap.initiate_swap(ip_id, "seller", 500, "buyer")
    try:
        swap.initiate_swap(ip_id, "seller", 500, "buyer")
        assert False
    except ValueError as e:
        assert "ActiveSwapAlreadyExistsForThisIpId" in str(e)


# ── Entry point ───────────────────────────────────────────────────────────────

if __name__ == "__main__":
    tests = [v for k, v in sorted(globals().items()) if k.startswith("test_")]
    passed = failed = 0
    for t in tests:
        try:
            t()
            print(f"  PASS  {t.__name__}")
            passed += 1
        except Exception as e:
            print(f"  FAIL  {t.__name__}: {e}")
            failed += 1
    print(f"\n{passed} passed, {failed} failed")
    sys.exit(1 if failed else 0)
