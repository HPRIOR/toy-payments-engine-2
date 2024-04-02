use std::{error::Error, ffi::OsString, fs::File};

use ::serde::{Deserialize, Serialize, Serializer};
use rust_decimal::Decimal;

use crate::types::{
    AccountActivity, ClientId, ClientLedger, DisputeManagement, MonetaryAmount,
    Transaction, TransactionId,
};

#[derive(Debug, Deserialize)]
pub enum TxTypeEntity {
    #[serde(alias = "deposit")]
    Deposit,
    #[serde(alias = "withdrawal")]
    Withdrawal,
    #[serde(alias = "dispute")]
    Dispute,
    #[serde(alias = "resolve")]
    Resolve,
    #[serde(alias = "chargeback")]
    ChargeBack,
}

#[derive(Debug, Deserialize)]
pub struct TxRowEntity {
    #[serde(alias = "type")]
    pub tx_type: TxTypeEntity,
    pub client: u16,
    pub tx: u32,
    pub amount: Option<f64>,
}

impl TxRowEntity {
    fn into_domain(self) -> Transaction {
        match self {
            TxRowEntity {
                tx_type: TxTypeEntity::Deposit,
                client,
                tx,
                amount: Some(a),
            } => Transaction::Activity(AccountActivity::Deposit(
                ClientId::new(client),
                TransactionId::new(tx),
                MonetaryAmount::new(a),
            )),
            TxRowEntity {
                tx_type: TxTypeEntity::Withdrawal,
                client,
                tx,
                amount: Some(a),
            } => Transaction::Activity(AccountActivity::Withdrawal(
                ClientId::new(client),
                TransactionId::new(tx),
                MonetaryAmount::new(a),
            )),
            TxRowEntity {
                tx_type: TxTypeEntity::Dispute,
                client,
                tx,
                amount: None,
            } => Transaction::Dispute(DisputeManagement::Dispute(
                ClientId::new(client),
                TransactionId::new(tx),
            )),
            TxRowEntity {
                tx_type: TxTypeEntity::Resolve,
                client,
                tx,
                amount: None,
            } => Transaction::Dispute(DisputeManagement::Resolve(
                ClientId::new(client),
                TransactionId::new(tx),
            )),
            TxRowEntity {
                tx_type: TxTypeEntity::ChargeBack,
                client,
                tx,
                amount: None,
            } => Transaction::Dispute(DisputeManagement::Chargeback(
                ClientId::new(client),
                TransactionId::new(tx),
            )),
            _ => panic!("Found unexpected row in the input: {:?}", self),
        }
    }
}

fn fixed_width<S: Serializer>(x: &Decimal, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&format!("{:.4}", x))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientLedgerEntity {
    client: u16,
    #[serde(serialize_with = "fixed_width")]
    available: Decimal,
    #[serde(serialize_with = "fixed_width")]
    held: Decimal,
    #[serde(serialize_with = "fixed_width")]
    total: Decimal,
    locked: bool,
}

impl ClientLedgerEntity {
    pub fn from_ledger(ledger: ClientLedger) -> Self {
        Self {
            client: ledger.id.value(),
            available: ledger.available.value(),
            held: ledger.held.value(),
            total: ledger.total.value(),
            locked: ledger.is_locked,
        }
    }
}

pub fn process_csv(csv_path: &OsString) -> Result<Vec<Transaction>, Box<dyn Error>> {
    let file = File::open(csv_path)?;
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(file);

    let mut rows: Vec<Transaction> = Vec::new();
    for row in reader.deserialize::<TxRowEntity>() {
        // fail if  cannot deserialise, no point in incomplete ledger
        rows.push(row?.into_domain());
    }

    Ok(rows)
}

pub fn output_csv(client_ledger: Vec<ClientLedger>) -> Result<String, Box<dyn Error>> {
    let mut wtr = csv::Writer::from_writer(vec![]);

    for client in client_ledger {
        wtr.serialize(ClientLedgerEntity::from_ledger(client))?
    }

    wtr.flush()?;
    let data = String::from_utf8(wtr.into_inner()?)?;
    Ok(data)
}
