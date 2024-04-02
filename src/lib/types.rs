use std::ops::{Add, Sub};

use im::{HashMap, HashSet, Vector};
use rust_decimal::Decimal;

#[derive(Default, Hash, Eq, PartialEq, Clone, Copy)]
pub struct ClientId(u16);

impl ClientId {
    pub fn new(value: u16) -> Self {
        Self(value)
    }

    pub fn value(&self) -> u16 {
        self.0
    }
}

#[derive(Default, Hash, Eq, PartialEq, Clone, Copy)]
pub struct TransactionId(u32);

impl TransactionId {
    pub fn new(value: u32) -> Self {
        Self(value)
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Debug)]
pub struct MonetaryAmount(Decimal);

impl MonetaryAmount {
    pub fn new(value: f64) -> Self {
        Self(
            Decimal::from_f64_retain(value)
                .unwrap_or_else(||panic!("Failed to parse {:#?} into Decimal", value)),
        )
    }

    pub fn value(&self) -> Decimal {
        self.0
    }
}

impl Add for MonetaryAmount {
    type Output = MonetaryAmount;

    fn add(self, rhs: Self) -> Self::Output {
        MonetaryAmount(self.value() + rhs.value())
    }
}

impl Sub for MonetaryAmount {
    type Output = MonetaryAmount;

    fn sub(self, rhs: Self) -> Self::Output {
        MonetaryAmount(self.value() - rhs.value())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum AccountActivity {
    /// Increases available and total funds by an amount.
    Deposit(ClientId, TransactionId, MonetaryAmount),
    /// Decreases available and total funds by an amount.
    ///
    /// Withdrawals can only taken against accounts with sufficient available funds. Withdrawals
    /// against a disputed account may be enacted after a dispute is resolved, if the resolution
    /// provides sufficient available funds.
    Withdrawal(ClientId, TransactionId, MonetaryAmount),
}

pub enum DisputeManagement {
    /// Decreases available funds and increases held funds by the amount of the transaction indicated by the transaction id.
    ///
    /// Disputes against non-existing transaction or client will be ignored. Since disputes
    /// _decrease_ available funds, only deposit transactions can be disputed (see readme regarding
    /// assumptions made).
    Dispute(ClientId, TransactionId),
    /// Releases available funds and decreases held funds. Failed withdrawals will be backfilled
    /// against the non-disputed available funds.
    Resolve(ClientId, TransactionId),
    /// Decreases held and total funds decrease by the disputed amount, and the account is frozen
    Chargeback(ClientId, TransactionId),
}

pub enum Transaction {
    Activity(AccountActivity),
    Dispute(DisputeManagement),
}

/// Stores a transaction that has failed, and any disputes that have occured prior to the failed
/// transaction. When disputed transactions are resolved this can be used to backfil failed
/// transactions.
#[derive(Clone)]
pub struct RejectedActivity {
    pub activity: AccountActivity,
    pub disputed_transaction_snapshot: HashSet<TransactionId>,
}

/// Contains data relating to previous transactions. A record of deposit and withdrawal transactions are kept for
/// the use by resolve, dispute and chargeback transactions.
/// Records of disputed and rejectedtransactions are stored so that previously rejected transactions can be backfilled.
#[derive(Default, Clone)]
pub struct TransactionHistory {
    pub account_activity: HashMap<TransactionId, AccountActivity>,
    pub disputed_txs: HashSet<TransactionId>,
    pub rejected_txs: Vector<RejectedActivity>,
}

impl TransactionHistory {
    pub fn map_account_activity<F>(&self, f: F) -> Self
    where
        F: FnOnce(
            &HashMap<TransactionId, AccountActivity>,
        ) -> HashMap<TransactionId, AccountActivity>,
    {
        Self {
            account_activity: f(&self.account_activity),
            ..self.clone()
        }
    }

    pub fn map_disputed_tx<F>(&self, f: F) -> Self
    where
        F: FnOnce(&HashSet<TransactionId>) -> HashSet<TransactionId>,
    {
        Self {
            disputed_txs: f(&self.disputed_txs),
            ..self.clone()
        }
    }

    pub fn map_rejected_activity<F>(&self, f: F) -> Self
    where
        F: FnOnce(&Vector<RejectedActivity>) -> Vector<RejectedActivity>,
    {
        Self {
            rejected_txs: f(&self.rejected_txs),
            ..self.clone()
        }
    }
}

#[derive(Default, Clone)]
pub struct ClientState {
    pub available: MonetaryAmount,
    pub held: MonetaryAmount,
    pub total: MonetaryAmount,
    pub is_locked: bool,
    pub history: TransactionHistory,
}

impl ClientState {
    pub fn map_avail<F: FnOnce(MonetaryAmount) -> MonetaryAmount>(&self, f: F) -> Self {
        Self {
            available: f(self.available),
            ..self.clone()
        }
    }

    pub fn map_total<F: FnOnce(MonetaryAmount) -> MonetaryAmount>(&self, f: F) -> Self {
        Self {
            total: f(self.total),
            ..self.clone()
        }
    }

    pub fn map_held<F: FnOnce(MonetaryAmount) -> MonetaryAmount>(&self, f: F) -> Self {
        Self {
            held: f(self.held),
            ..self.clone()
        }
    }

    pub fn map_history<F: FnOnce(&TransactionHistory) -> TransactionHistory>(&self, f: F) -> Self {
        Self {
            history: f(&self.history),
            ..self.clone()
        }
    }

    pub fn update_locked(&self, is_locked: bool) -> Self {
        Self {
            is_locked,
            ..self.clone()
        }
    }
}

pub struct ClientLedger {
    pub id: ClientId,
    pub available: MonetaryAmount,
    pub held: MonetaryAmount,
    pub total: MonetaryAmount,
    pub is_locked: bool,
}

impl ClientLedger {
    pub fn from_state(id: ClientId, state: ClientState) -> Self {
        Self {
            id,
            available: state.available,
            held: state.held,
            total: state.total,
            is_locked: state.is_locked,
        }
    }
}

#[derive(Default)]
pub struct Ledger(pub Vec<ClientLedger>);
