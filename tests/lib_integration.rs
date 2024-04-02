use std::ffi::OsString;

use test_utils::{assert_unsorted_eq, create_csv};
use toy_payments_lib::process_payments;

extern crate test_utils;

#[test]
fn basic_example() {
    let sut = process_payments(&OsString::from("tests/resources/basic_example.csv")).unwrap();
    let expected = create_csv(vec![
        ["1", "1.5000", "0.0000", "1.5000", "false"],
        ["2", "2.0000", "0.0000", "2.0000", "false"],
    ]);
    assert_unsorted_eq(&sut, &expected);
}

#[test]
fn cannot_withdraw_over_avail() {
    let sut = process_payments(&OsString::from("tests/resources/withdraw_over_avail.csv")).unwrap();
    let expected = create_csv(vec![["1", "20.0000", "0.0000", "20.0000", "false"]]);
    assert_eq!(sut, expected)
}

#[test]
fn false_disputes_are_ignored() {
    let sut = process_payments(&OsString::from("tests/resources/false_disputes.csv")).unwrap();
    let expected = create_csv(vec![["1", "80.0000", "0.0000", "80.0000", "false"]]);
    assert_eq!(sut, expected)
}

#[test]
fn disputes_correctly_modify_account() {
    let sut = process_payments(&OsString::from("tests/resources/dispute_example.csv")).unwrap();
    let expected = create_csv(vec![["1", "0.0000", "150.0000", "150.0000", "false"]]);
    assert_eq!(sut, expected)
}

#[test]
fn resolves_correctly_modify_account() {
    let sut = process_payments(&OsString::from("tests/resources/resolve_example.csv")).unwrap();
    let expected = create_csv(vec![["1", "240.0000", "0.0000", "240.0000", "false"]]);
    assert_eq!(sut, expected)
}

#[test]
fn false_resolves_are_ignored() {
    let sut = process_payments(&OsString::from("tests/resources/false_resolves.csv")).unwrap();
    let expected = create_csv(vec![["1", "40.0000", "200.0000", "240.0000", "false"]]);
    assert_eq!(sut, expected)
}

// Assumption: withdraws that occur after a dispute can be enacted post-hoc if they are within
// the limits of available funds after resolution
#[test]
fn withdrawals_retroactively_resolved() {
    let sut = process_payments(&OsString::from("tests/resources/retroactive_resolve.csv")).unwrap();
    let expected = create_csv(vec![["1", "0.0000", "0.0000", "0.0000", "false"]]);
    assert_eq!(sut, expected)
}

// Assumption: retroactive resolution of withdrawals only apply to withdraws rejected
// after a dispute. If a transaction occurs before a dispute and is rejected, then it is based on
// the available funds at the time of the transaction, and only retroactively
// dependant on the disputes occurring prior to it
#[test]
fn no_retroactive_resolve_for_withdraw_prior_to_dispute() {
    let sut = process_payments(&OsString::from(
        "tests/resources/retroactive_resolve_with_rejected_withdrawal.csv",
    ))
    .unwrap();
    let expected = create_csv(vec![["1", "200.0000", "50.0000", "250.0000", "false"]]);
    assert_eq!(sut, expected)
}

#[test]
fn false_chargebacks_are_ignored() {
    let sut = process_payments(&OsString::from("tests/resources/false_chargebacks.csv")).unwrap();
    let expected = create_csv(vec![["1", "50.0000", "0.0000", "50.0000", "false"]]);
    assert_eq!(sut, expected)
}

#[test]
fn chargeback_will_block_account_and_reduce_funds() {
    let sut = process_payments(&OsString::from("tests/resources/upheld_chargeback.csv")).unwrap();
    let expected = create_csv(vec![["1", "-50.0000", "0.0000", "-50.0000", "true"]]);
    assert_eq!(sut, expected)
}
