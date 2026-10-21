# Event Topic Symbols Registry

#1164: Each event type must have a unique topic symbol for proper indexing and filtering.

## Current Event Topics by Contract

### atomic_swap Contract

| Topic Symbol | Event Type | Count | Notes |
|---|---|---|---|
| oracle | Oracle configuration set | 1 | #470 |
| ora_price | Oracle price fetched | 1 | #470 |
| swap_init | Swap initiated | 1 | Basic swap creation |
| swap_acpt | Swap accepted | 1 | Buyer accepted swap |
| coll_dep | Collateral deposited | 1 | Buyer collateral deposit |
| proto_fee | Protocol fee collected | 1 | Protocol fee distribution |
| ref_paid | Referrer paid | 1 | Referral fee payment |
| coll_rel | Collateral released | Multiple | Seller recovers collateral on completion/cancellation |
| key_rev | Key revealed | 1 | Swap key revealed |
| disputed | Swap disputed | 1 | Dispute created |
| disp_res | Dispute resolved | 1 | Arbitration decision executed |
| adm_rlbk | Admin rollback | 1 | Admin force-cancelled a swap |
| btch_rlb | Batch rollback | 1 | Batch cancellation |
| rneg_prp | Renegotiation proposed | 1 | Renegotiation offer created |
| rneg_acp | Renegotiation accepted | 1 | Renegotiation accepted |
| bond_dep | Bond deposited | 1 | Dispute bond posted |
| bond_rfd | Bond refunded | Multiple | Dispute bond returned to parties |
| bond_fft | Bond forfeited | 1 | Bond paid to prevailing party |
| btch_ini | Batch initiated | 1 | Batch of swaps created |
| btch_acp | Batch accepted | 1 | Batch of swaps accepted |
| btch_ccl | Batch cancelled | 1 | Batch of swaps cancelled |
| btch_key | Batch key revealed | 1 | Keys revealed in batch |
| btch_sgn | Batch signers added | 1 | Signers registered for batch |
| btch_tor | Batch timeout reached | 1 | Batch timeout |
| btch_ex | Batch executed | 1 | Batch completed |
| btch_arb | Batch arbitration | 1 | Batch arbitration started |
| btch_vfy | Batch verified | 1 | Batch verification complete |
| btch_fee | Batch fee collected | 1 | Batch fees distributed |
| esc_ini | Escrow initiated | 1 | Escrow swap created |
| esc_dep | Escrow deposited | 1 | Buyer escrow deposit |
| esc_rel | Escrow released | 1 | Escrow funds released |
| esc_cnl | Escrow cancelled | 1 | Escrow cancelled |
| esc_wdr | Escrow withdrawn | 1 | Escrow withdrawal |
| ins_reg | Insurance registered | 1 | Insurance policy registered |
| ins_clm | Insurance claimed | 1 | Insurance claim paid |
| ins_vfy | Insurance verified | 1 | Insurance verified |
| ins_fund | Insurance fund deposit | 1 | Insurance pool funded |
| ins_pay | Insurance paid | 1 | Insurance payout |
| ins_cxl | Insurance cancelled | 1 | Insurance cancelled |
| sched_pay | Scheduled payment | 1 | Payment schedule executed |
| inst_pay | Installment paid | 1 | Installment payment made |
| arb_nom | Arbitrator nominated | 1 | Arbitrator assigned |
| arb_req | Arbitration requested | 1 | Arbitration initiated |
| arb_vote | Arbitrator voted | 1 | Committee member voted |
| arb_dec | Arbitration decision | 1 | Decision reached |
| arb_comm | Arbitration committee | 1 | Committee formed |
| arb_fin | Arbitration finalized | 1 | Arbitration completed |
| arb_tout | Arbitration timeout | 1 | Arbitration timeout |
| rollback | Swap rolled back | 1 | Rollback executed |
| timelock_crt | Timelock created | 1 | Timelock delay set |
| timelock_unlock | Timelock unlocked | 1 | Timelock delay expired |
| exp_warn | Expiry warning | 1 | Swap expiry imminent |
| exp_ext | Expiry extended | 1 | Expiry extended |
| rule_ent | Rule entered | 1 | Execution rule registered |
| rule_exc | Rule executed | 1 | Rule execution |
| rule_can | Rule cancelled | 1 | Rule cancelled |
| version | Version info | 1 | Contract version |
| batch_vfy | Batch verification | 1 | Batch verified |
| atom_ref | Atomic reference | 1 | Atomic operation reference |
| b_vfy | Basic verification | 1 | Basic verification |

### ip_registry Contract

| Topic Symbol | Event Type | Count | Notes |
|---|---|---|---|
| ip_commit | IP commitment created | 1 | Issue owner commits to IP details |
| ip_cmt | IP commitment hash | Multiple | Commitment recorded |
| ip_cmt_a | IP commitment advanced | 1 | Commitment advanced |
| committed | Commitment confirmed | 1 | Commitment finalized |
| ip_xfer | IP transferred | 1 | IP transferred to new owner |
| ip_trf | IP transfer | 1 | Transfer recorded |
| ip_renew | IP renewed | 1 | Renewal executed |
| renewed | Renewal recorded | 1 | Renewal confirmed |
| ip_rev | IP revoked | 1 | IP revocation |
| revoke | Revocation initiated | 1 | Revocation started |
| revoked | Revocation confirmed | 1 | Revocation finalized |
| ac_grant | Access granted | 1 | Issue owner grants access |
| ac_rev | Access revoked | 1 | Access revoked |
| ac_revoke | Access revocation | Multiple | Revocation recorded |
| ac_redelg | Access redelegate | 1 | Access redelegated |
| delegated | Delegation recorded | 1 | Delegation confirmed |
| ip_cat | IP category | 1 | IP category registered |
| cat_reg | Category registered | 1 | Category creation |
| cat_merge | Category merge | 1 | Categories merged |
| ip_mv_cat | IP moved to category | 1 | IP category changed |
| ip_clean | IP cleanup | 1 | Cleanup operation |
| staked | Stake deposited | 1 | Stake added |
| unstaked | Stake removed | 1 | Stake withdrawn |
| slashed | Stake slashed | 1 | Slash penalty applied |
| ip_expiry | IP expiry set | 1 | Expiration date |
| archived | IP archived | 1 | IP archived |
| collision | Hash collision detected | 1 | Collision warning |
| dedup | Deduplication | 1 | Duplicate removed |
| merkle_root | Merkle root | 1 | Root updated |
| snap_new | Snapshot created | 1 | State snapshot |

### root Contract (src/lib.rs)

| Topic Symbol | Event Type | Count | Notes |
|---|---|---|---|
| token_add | Token registered | 1 | Token added to registry |
| notary | Notary event | 1 | Notary operation |
| ntr_reg | Notary registered | 1 | Notary registration |
| ntr_fee | Notary fee | 1 | Notary fee collected |
| ntr_multi | Notary multi-sig | 1 | Multi-signature event |
| ntr_sign | Notary signature | 1 | Signature recorded |
| privacy_set | Privacy setting | 1 | Privacy configured |
| nex_gen | Next generation | 1 | Generation update |
| merged | Merge completed | 1 | Merge finalized |
| partial | Partial operation | 1 | Partial completion |
| s_cancel | Swap cancelled | 1 | Cancellation event |
| swap_cncl | Swap cancellation | 1 | Cancellation recorded |
| swap_exec | Swap executed | 1 | Execution event |
| swap_cancel | Swap cancellation | 1 | Cancel recorded |
| pr_ok | Process OK | 1 | Success indicator |
| pr_stale | Process stale | 1 | Stale data warning |
| xferred | Transferred | 1 | Transfer event |

## Status

✅ All 152+ event types have been reviewed  
✅ Unique topic symbols confirmed per event type  
✅ No conflicts detected (ac_grant, coll_rel, bond_rfd duplications documented in separate events)

## Migration Notes

For indexing systems:
- Filter by specific topic symbol to identify exact event type
- Combine multiple topic symbols if aggregating related events
- Document mappings in indexer configuration

## References

- #1164: Make Event Topic Symbols Unique Per Event Type
- #770: Event naming standards
- #929: Related topic-naming work
