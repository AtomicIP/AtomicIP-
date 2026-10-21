//! IP transfer facilitation service (#1093).
//!
//! Guides an IP transfer through a validated, escrow-backed workflow:
//!
//! ```text
//! Proposed --validate--> Validated --fund escrow--> Funded --(automatic)--> Settled
//!     |                      |
//!     +------cancel----------+--------> Cancelled
//! ```
//!
//! Once the full price is escrowed, settlement runs automatically in the same
//! step, so a funded transfer is never left stranded.

use axum::{extract::Path, http::StatusCode, Json};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::handlers::now_timestamp;
use crate::schemas::ErrorResponse;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    Proposed,
    Validated,
    Funded,
    Settled,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transfer {
    pub id: u64,
    pub ip_id: u64,
    pub from: String,
    pub to: String,
    /// Agreed price in stroops.
    pub price: i128,
    pub status: TransferStatus,
    /// Amount currently held in escrow, in stroops.
    pub escrow_amount: i128,
    pub created_at: u64,
    pub settled_at: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTransferRequest {
    pub ip_id: u64,
    pub from: String,
    pub to: String,
    pub price: i128,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FundEscrowRequest {
    pub amount: i128,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TransferError {
    NotFound(u64),
    Invalid(String),
    InvalidState(String),
}

impl TransferError {
    fn status(&self) -> StatusCode {
        match self {
            TransferError::NotFound(_) => StatusCode::NOT_FOUND,
            TransferError::Invalid(_) => StatusCode::BAD_REQUEST,
            TransferError::InvalidState(_) => StatusCode::CONFLICT,
        }
    }

    fn message(&self) -> String {
        match self {
            TransferError::NotFound(id) => format!("transfer {id} not found"),
            TransferError::Invalid(msg) | TransferError::InvalidState(msg) => msg.clone(),
        }
    }
}

static TRANSFERS: Lazy<DashMap<u64, Transfer>> = Lazy::new(DashMap::new);
static NEXT_TRANSFER_ID: AtomicU64 = AtomicU64::new(1);

/// Open a new transfer proposal for an IP between two distinct parties.
pub fn create(req: CreateTransferRequest, now: u64) -> Result<Transfer, TransferError> {
    if req.from.trim().is_empty() || req.to.trim().is_empty() {
        return Err(TransferError::Invalid("from and to must not be empty".to_string()));
    }
    if req.from == req.to {
        return Err(TransferError::Invalid("from and to must be different parties".to_string()));
    }
    if req.price <= 0 {
        return Err(TransferError::Invalid("price must be positive".to_string()));
    }
    let transfer = Transfer {
        id: NEXT_TRANSFER_ID.fetch_add(1, Ordering::Relaxed),
        ip_id: req.ip_id,
        from: req.from,
        to: req.to,
        price: req.price,
        status: TransferStatus::Proposed,
        escrow_amount: 0,
        created_at: now,
        settled_at: None,
    };
    TRANSFERS.insert(transfer.id, transfer.clone());
    Ok(transfer)
}

pub fn get(id: u64) -> Option<Transfer> {
    TRANSFERS.get(&id).map(|t| t.clone())
}

/// Mark a proposal as validated, ready for escrow funding.
pub fn validate(id: u64) -> Result<Transfer, TransferError> {
    transition(id, |t| {
        if t.status != TransferStatus::Proposed {
            return Err(TransferError::InvalidState(format!(
                "only a proposed transfer can be validated (status is {:?})",
                t.status
            )));
        }
        t.status = TransferStatus::Validated;
        Ok(())
    })
}

/// Deposit the full price into escrow. Settlement follows automatically once fully funded.
pub fn fund_escrow(id: u64, amount: i128, now: u64) -> Result<Transfer, TransferError> {
    transition(id, |t| {
        if t.status != TransferStatus::Validated {
            return Err(TransferError::InvalidState(format!(
                "escrow can only be funded for a validated transfer (status is {:?})",
                t.status
            )));
        }
        if amount != t.price {
            return Err(TransferError::Invalid(format!(
                "escrow amount must equal the agreed price of {}",
                t.price
            )));
        }
        t.escrow_amount = amount;
        t.status = TransferStatus::Funded;
        // Automatic settlement: the full price is now escrowed, so settle in this same step.
        settle_funded(t, now);
        Ok(())
    })
}

fn settle_funded(t: &mut Transfer, now: u64) {
    debug_assert_eq!(t.status, TransferStatus::Funded);
    t.status = TransferStatus::Settled;
    t.settled_at = Some(now);
}

/// Cancel a transfer before escrow is funded. Funded or settled transfers cannot be cancelled.
pub fn cancel(id: u64) -> Result<Transfer, TransferError> {
    transition(id, |t| {
        match t.status {
            TransferStatus::Proposed | TransferStatus::Validated => {
                t.status = TransferStatus::Cancelled;
                Ok(())
            }
            other => Err(TransferError::InvalidState(format!(
                "transfer cannot be cancelled once it is {other:?}"
            ))),
        }
    })
}

fn transition(
    id: u64,
    apply: impl FnOnce(&mut Transfer) -> Result<(), TransferError>,
) -> Result<Transfer, TransferError> {
    let mut entry = TRANSFERS.get_mut(&id).ok_or(TransferError::NotFound(id))?;
    apply(&mut entry)?;
    Ok(entry.clone())
}

fn respond(result: Result<Transfer, TransferError>) -> Result<Json<Transfer>, (StatusCode, Json<ErrorResponse>)> {
    result.map(Json).map_err(|err| {
        (err.status(), Json(ErrorResponse { error: err.message() }))
    })
}

/// `POST /v1/transfers` — propose an IP transfer.
pub async fn create_transfer(
    Json(body): Json<CreateTransferRequest>,
) -> Result<(StatusCode, Json<Transfer>), (StatusCode, Json<ErrorResponse>)> {
    match create(body, now_timestamp()) {
        Ok(transfer) => Ok((StatusCode::CREATED, Json(transfer))),
        Err(err) => Err((err.status(), Json(ErrorResponse { error: err.message() }))),
    }
}

/// `GET /v1/transfers/{id}` — current state of a transfer.
pub async fn get_transfer(Path(id): Path<u64>) -> Result<Json<Transfer>, (StatusCode, Json<ErrorResponse>)> {
    respond(get(id).ok_or(TransferError::NotFound(id)))
}

/// `POST /v1/transfers/{id}/validate` — validate a proposed transfer.
pub async fn validate_transfer(Path(id): Path<u64>) -> Result<Json<Transfer>, (StatusCode, Json<ErrorResponse>)> {
    respond(validate(id))
}

/// `POST /v1/transfers/{id}/escrow` — fund escrow, which triggers automatic settlement.
pub async fn fund_transfer_escrow(
    Path(id): Path<u64>,
    Json(body): Json<FundEscrowRequest>,
) -> Result<Json<Transfer>, (StatusCode, Json<ErrorResponse>)> {
    respond(fund_escrow(id, body.amount, now_timestamp()))
}

/// `POST /v1/transfers/{id}/cancel` — cancel a transfer before escrow is funded.
pub async fn cancel_transfer(Path(id): Path<u64>) -> Result<Json<Transfer>, (StatusCode, Json<ErrorResponse>)> {
    respond(cancel(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(price: i128) -> CreateTransferRequest {
        CreateTransferRequest {
            ip_id: 42,
            from: "GSELLER".to_string(),
            to: "GBUYER".to_string(),
            price,
        }
    }

    #[test]
    fn full_workflow_settles_automatically_on_funding() {
        let created = create(request(1_000), 100).unwrap();
        assert_eq!(created.status, TransferStatus::Proposed);

        let validated = validate(created.id).unwrap();
        assert_eq!(validated.status, TransferStatus::Validated);

        let settled = fund_escrow(created.id, 1_000, 200).unwrap();
        assert_eq!(settled.status, TransferStatus::Settled);
        assert_eq!(settled.escrow_amount, 1_000);
        assert_eq!(settled.settled_at, Some(200));
    }

    #[test]
    fn cannot_fund_escrow_before_validation() {
        let created = create(request(500), 1).unwrap();
        let err = fund_escrow(created.id, 500, 2).unwrap_err();
        assert!(matches!(err, TransferError::InvalidState(_)));
    }

    #[test]
    fn escrow_must_match_agreed_price() {
        let created = create(request(500), 1).unwrap();
        validate(created.id).unwrap();
        let err = fund_escrow(created.id, 499, 2).unwrap_err();
        assert!(matches!(err, TransferError::Invalid(_)));
        assert_eq!(get(created.id).unwrap().status, TransferStatus::Validated);
    }

    #[test]
    fn cancel_is_allowed_before_funding_only() {
        let open = create(request(10), 1).unwrap();
        assert_eq!(cancel(open.id).unwrap().status, TransferStatus::Cancelled);

        let done = create(request(10), 1).unwrap();
        validate(done.id).unwrap();
        fund_escrow(done.id, 10, 2).unwrap();
        assert!(matches!(cancel(done.id), Err(TransferError::InvalidState(_))));
    }

    #[test]
    fn rejects_invalid_proposals() {
        let mut same = request(10);
        same.to = same.from.clone();
        assert!(matches!(create(same, 1), Err(TransferError::Invalid(_))));
        assert!(matches!(create(request(0), 1), Err(TransferError::Invalid(_))));
    }

    #[test]
    fn unknown_transfer_is_not_found() {
        assert_eq!(validate(u64::MAX), Err(TransferError::NotFound(u64::MAX)));
    }
}
