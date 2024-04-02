mod io;
mod transactions;
mod types;
mod utils;

use std::{error::Error, ffi::OsString};

use io::{output_csv, process_csv};
use transactions::create_ledger;

pub fn process_payments(csv_path: &OsString) -> Result<String, Box<dyn Error>> {
    let transactions = process_csv(csv_path)?;

    let ledger = create_ledger(Box::new(transactions.into_iter()));

    let result = output_csv(ledger.0)?;
    Ok(result)
}
