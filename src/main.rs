use std::{env, process};
use toy_payments_lib::process_payments;

fn main() {
    match env::args_os().nth(1) {
        None => {
            eprintln!("Missing csv file argument");
            process::exit(1);
        }
        Some(csv_path) => {
            match process_payments(&csv_path) {
                Ok(result) => {
                    println!("{}", result);
                    process::exit(0);
                }
                Err(e) => {
                    // error occurred
                    eprintln!("an error occurred: {:#?}", e);
                    process::exit(1);
                }
            }
        }
    }
}
