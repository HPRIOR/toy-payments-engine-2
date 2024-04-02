use crate::types::{
    AccountActivity, ClientId, ClientLedger, ClientState, DisputeManagement, Ledger,
    MonetaryAmount, RejectedActivity, Transaction, TransactionId,
};
use crate::utils::{OrDefault, PushImmut, RemoveImmut};
use im::HashMap;

fn update_deposit(
    client_state: ClientState,
    activity: &AccountActivity,
    tx_id: TransactionId,
    amount: MonetaryAmount,
) -> ClientState {
    if client_state.is_locked {
        return client_state;
    }
    client_state
        .map_avail(|a| a + amount)
        .map_total(|t| t + amount)
        .map_history(|h| {
            h.map_account_activity(|account_acc| account_acc.update(tx_id, activity.clone()))
        })
}

fn update_withdrawal(
    client_state: ClientState,
    activity: &AccountActivity,
    tx_id: TransactionId,
    amount: MonetaryAmount,
) -> ClientState {
    // The resolutoin of disputes will not effect this transaction
    let no_possible_withdrawal_backfill = (client_state.available < amount
        && client_state.history.disputed_txs.is_empty())
        || client_state.total < amount;

    if client_state.is_locked || no_possible_withdrawal_backfill {
        return client_state;
    };

    // The resolutoin of disputes may effect this transaction
    let potential_backfill =
        client_state.available < amount && !client_state.history.disputed_txs.is_empty();

    if potential_backfill {
        let disputed_transaction_snapshot = client_state.history.disputed_txs.clone();
        let rejected_activity = RejectedActivity {
            activity: activity.clone(),
            disputed_transaction_snapshot,
        };
        client_state.map_history(|h| h.map_rejected_activity(|r| r.push(rejected_activity)))
    } else {
        client_state
            .map_total(|t| t - amount)
            .map_avail(|a| a - amount)
            .map_history(|h| {
                h.map_account_activity(|account_acc| account_acc.update(tx_id, activity.clone()))
            })
    }
}

fn update_dispute(client_state: ClientState, tx_id: TransactionId) -> Option<ClientState> {
    let is_already_disputed = client_state.history.disputed_txs.contains(&tx_id);
    if client_state.is_locked || is_already_disputed {
        return None;
    }

    let maybe_tx_amount = client_state.history.account_activity.get(&tx_id);
    // Only deposits can be disputed (see readme).
    if let Some(AccountActivity::Deposit(_, tx_id, amount)) = maybe_tx_amount {
        Some(
            client_state
                .map_avail(|a| a - *amount)
                .map_held(|h| h + *amount)
                .map_history(|history| history.map_disputed_tx(|disputed| disputed.update(*tx_id))),
        )
    } else {
        None
    }
}

fn resolve_prev_rejected(resolved_tx: TransactionId, client_state: ClientState) -> ClientState {
    client_state
        .history
        .rejected_txs
        .iter()
        .fold(client_state.clone(), |acc, rejected_tx| {
            // Rejected transactions store all disputes that occured prior to their rejection. If
            // the current resolved_tx is present here, the client may now have sufficient
            // avaiable funds to enact the transaction
            let rejected_tx_occured_before_resolved_tx = rejected_tx
                .disputed_transaction_snapshot
                .contains(&resolved_tx);

            let withdraw_amount =
                if let AccountActivity::Withdrawal(_, _, amount) = rejected_tx.activity {
                    amount
                } else {
                    panic!("Only withdrawals can be backfilled");
                };

            let withdraw_within_avail = withdraw_amount <= acc.available;

            if rejected_tx_occured_before_resolved_tx && withdraw_within_avail {
                // Previous rejected transaction is resolved
                acc.map_avail(|a| a - withdraw_amount)
                    .map_total(|t| t - withdraw_amount)
                    // Rejected transaction is removed from history so that it is not processed twice
                    .map_history(|h| {
                        // this is proibably quite slow if
                        h.map_rejected_activity(|rej| {
                            let idx = rej
                                .into_iter()
                                .enumerate()
                                .find(|(_, x)| x.activity == rejected_tx.activity)
                                .map(|(i, _)| i)
                                .unwrap();
                            rej.remove_idx(idx)
                        })
                    })
            } else {
                acc.clone()
            }
        })
}

fn update_resolve(client_state: ClientState, tx_id: TransactionId) -> Option<ClientState> {
    let is_disputed = client_state.history.disputed_txs.contains(&tx_id);
    if client_state.is_locked || !is_disputed {
        return None;
    }
    let maybe_tx_amount = client_state.history.account_activity.get(&tx_id);
    if let Some(AccountActivity::Deposit(_, tx_id, amount)) = maybe_tx_amount {
        let new_state = client_state
            .map_avail(|a| a + *amount)
            .map_held(|h| h - *amount)
            .map_history(|h| h.map_disputed_tx(|disputed| disputed.without(tx_id)));

        Some(resolve_prev_rejected(*tx_id, new_state))
    } else {
        None
    }
}

fn update_chargeback(client_state: ClientState, tx_id: TransactionId) -> Option<ClientState> {
    let is_disputed = client_state.history.disputed_txs.contains(&tx_id);
    if client_state.is_locked || !is_disputed {
        return None;
    }
    let maybe_tx_amount = client_state.history.account_activity.get(&tx_id);
    if let Some(AccountActivity::Deposit(_, _, amount)) = maybe_tx_amount {
        Some(
            client_state
                .map_total(|t| t - *amount)
                .map_held(|h| h - *amount)
                .update_locked(true),
        )
    } else {
        None
    }
}

fn resolve_transaction(
    transaction: Transaction,
    ledger: HashMap<ClientId, ClientState>,
) -> HashMap<ClientId, ClientState> {
    match transaction {
        Transaction::Activity(ref activity @ AccountActivity::Deposit(c_id, tx_id, amount)) => {
            let client_state = ledger.get_or_default(&c_id);
            let new_state = update_deposit(client_state, activity, tx_id, amount);
            ledger.update(c_id, new_state)
        }
        Transaction::Activity(ref activity @ AccountActivity::Withdrawal(c_id, tx_id, amount)) => {
            let client_state = ledger.get_or_default(&c_id);
            let new_state = update_withdrawal(client_state, activity, tx_id, amount);
            ledger.update(c_id, new_state)
        }
        Transaction::Dispute(DisputeManagement::Dispute(c_id, tx_id)) => {
            let client_state = ledger.get_or_default(&c_id);
            let new_state = update_dispute(client_state, tx_id);
            match new_state {
                Some(state) => ledger.update(c_id, state),
                None => ledger,
            }
        }
        Transaction::Dispute(DisputeManagement::Resolve(c_id, tx_id)) => {
            let client_state = ledger.get_or_default(&c_id);
            let new_state = update_resolve(client_state, tx_id);
            match new_state {
                Some(state) => ledger.update(c_id, state),
                None => ledger,
            }
        }
        Transaction::Dispute(DisputeManagement::Chargeback(c_id, tx_id)) => {
            let client_state = ledger.get_or_default(&c_id);
            let new_state = update_chargeback(client_state, tx_id);
            match new_state {
                Some(state) => ledger.update(c_id, state),
                None => ledger,
            }
        }
    }
}

// Used for testing
fn create_ledger_with_init(
    init_ledger: HashMap<ClientId, ClientState>,
    transactions: Box<dyn Iterator<Item = Transaction>>,
) -> Ledger {
    Ledger(
        transactions
            .fold(init_ledger, |acc, tx| resolve_transaction(tx, acc))
            .into_iter()
            .map(|(k, v)| ClientLedger::from_state(k, v))
            .collect(),
    )
}

// public interface
pub fn create_ledger(transactions: Box<dyn Iterator<Item = Transaction>>) -> Ledger {
    create_ledger_with_init(HashMap::default(), transactions)
}

#[cfg(test)]
mod tests {
    use crate::types::{
        AccountActivity, ClientId, ClientState, DisputeManagement, MonetaryAmount, Transaction,
        TransactionHistory, TransactionId,
    };
    use im::HashMap;

    use super::create_ledger_with_init;

    #[test]
    fn cannot_withdraw_under_avail() {
        let client_id = ClientId::new(1);

        let init_state = ClientState {
            total: MonetaryAmount::new(10.0),
            available: MonetaryAmount::new(5.0),
            held: MonetaryAmount::new(5.0),
            history: TransactionHistory::default(),
            is_locked: false,
        };
        let init_ledger: HashMap<ClientId, ClientState> =
            [(client_id, init_state.clone())].into_iter().collect();

        let transactions = vec![Transaction::Activity(AccountActivity::Withdrawal(
            client_id,
            TransactionId::new(1),
            MonetaryAmount::new(6.0),
        ))];

        let final_ledger = create_ledger_with_init(init_ledger, Box::new(transactions.into_iter()));

        let client_ledger = final_ledger
            .0
            .into_iter()
            .find(|x| x.id == client_id)
            .unwrap();

        assert_eq!(client_ledger.total, MonetaryAmount::new(10.0));
        assert_eq!(client_ledger.available, MonetaryAmount::new(5.0));
        assert_eq!(client_ledger.held, MonetaryAmount::new(5.0));
    }

    #[test]
    fn can_withdraw_within_avail() {
        let client_id = ClientId::new(1);

        let init_state = ClientState {
            total: MonetaryAmount::new(10.0),
            available: MonetaryAmount::new(5.0),
            held: MonetaryAmount::new(5.0),
            history: TransactionHistory::default(),
            is_locked: false,
        };
        let init_ledger: HashMap<ClientId, ClientState> =
            [(client_id, init_state.clone())].into_iter().collect();

        let transactions = vec![Transaction::Activity(AccountActivity::Withdrawal(
            client_id,
            TransactionId::new(1),
            MonetaryAmount::new(5.0),
        ))];

        let final_ledger = create_ledger_with_init(init_ledger, Box::new(transactions.into_iter()));

        let client_ledger = final_ledger
            .0
            .into_iter()
            .find(|x| x.id == client_id)
            .unwrap();

        assert_eq!(client_ledger.total, MonetaryAmount::new(5.0));
        assert_eq!(client_ledger.available, MonetaryAmount::new(0.0));
        assert_eq!(client_ledger.held, MonetaryAmount::new(5.0));
    }

    #[test]
    fn deposit_increases_total_and_avail() {
        let client_id = ClientId::new(1);

        let init_state = ClientState {
            total: MonetaryAmount::new(10.0),
            available: MonetaryAmount::new(5.0),
            held: MonetaryAmount::new(5.0),
            history: TransactionHistory::default(),
            is_locked: false,
        };
        let init_ledger: HashMap<ClientId, ClientState> =
            [(client_id, init_state.clone())].into_iter().collect();

        let transactions = vec![Transaction::Activity(AccountActivity::Deposit(
            client_id,
            TransactionId::new(1),
            MonetaryAmount::new(5.0),
        ))];

        let final_ledger = create_ledger_with_init(init_ledger, Box::new(transactions.into_iter()));

        let client_ledger = final_ledger
            .0
            .into_iter()
            .find(|x| x.id == client_id)
            .unwrap();

        assert_eq!(client_ledger.total, MonetaryAmount::new(15.0));
        assert_eq!(client_ledger.available, MonetaryAmount::new(10.0));
        assert_eq!(client_ledger.held, MonetaryAmount::new(5.0));
    }

    #[test]
    fn disputed_deposit_reduces_avail() {
        let client_id = ClientId::new(1);

        let init_state = ClientState {
            total: MonetaryAmount::new(10.0),
            available: MonetaryAmount::new(10.0),
            held: MonetaryAmount::new(0.0),
            history: TransactionHistory::default(),
            is_locked: false,
        };
        let init_ledger: HashMap<ClientId, ClientState> =
            [(client_id, init_state.clone())].into_iter().collect();

        let transactions = vec![
            Transaction::Activity(AccountActivity::Deposit(
                client_id,
                TransactionId::new(1),
                MonetaryAmount::new(5.0),
            )),
            Transaction::Dispute(DisputeManagement::Dispute(client_id, TransactionId::new(1))),
        ];

        let final_ledger = create_ledger_with_init(init_ledger, Box::new(transactions.into_iter()));

        let client_ledger = final_ledger
            .0
            .into_iter()
            .find(|x| x.id == client_id)
            .unwrap();

        assert_eq!(client_ledger.available, MonetaryAmount::new(10.0));
    }

    #[test]
    fn disputed_deposit_does_not_reduce_total() {
        let client_id = ClientId::new(1);

        let init_state = ClientState {
            total: MonetaryAmount::new(10.0),
            available: MonetaryAmount::new(10.0),
            held: MonetaryAmount::new(0.0),
            history: TransactionHistory::default(),
            is_locked: false,
        };
        let init_ledger: HashMap<ClientId, ClientState> =
            [(client_id, init_state.clone())].into_iter().collect();

        let transactions = vec![
            Transaction::Activity(AccountActivity::Deposit(
                client_id,
                TransactionId::new(1),
                MonetaryAmount::new(5.0),
            )),
            Transaction::Dispute(DisputeManagement::Dispute(client_id, TransactionId::new(1))),
        ];

        let final_ledger = create_ledger_with_init(init_ledger, Box::new(transactions.into_iter()));

        let client_ledger = final_ledger
            .0
            .into_iter()
            .find(|x| x.id == client_id)
            .unwrap();

        assert_eq!(client_ledger.total, MonetaryAmount::new(15.0));
    }

    #[test]
    fn dispute_will_increase_held_amount() {
        let client_id = ClientId::new(1);

        let init_state = ClientState {
            total: MonetaryAmount::new(10.0),
            available: MonetaryAmount::new(10.0),
            held: MonetaryAmount::new(0.0),
            history: TransactionHistory::default(),
            is_locked: false,
        };
        let init_ledger: HashMap<ClientId, ClientState> =
            [(client_id, init_state.clone())].into_iter().collect();

        let transactions = vec![
            Transaction::Activity(AccountActivity::Deposit(
                client_id,
                TransactionId::new(1),
                MonetaryAmount::new(5.0),
            )),
            Transaction::Dispute(DisputeManagement::Dispute(client_id, TransactionId::new(1))),
        ];

        let final_ledger = create_ledger_with_init(init_ledger, Box::new(transactions.into_iter()));

        let client_ledger = final_ledger
            .0
            .into_iter()
            .find(|x| x.id == client_id)
            .unwrap();

        assert_eq!(client_ledger.held, MonetaryAmount::new(5.0));
    }

    #[test]
    fn disputes_against_withdrawals_are_ignored() {
        let client_id = ClientId::new(1);

        let init_state = ClientState {
            total: MonetaryAmount::new(10.0),
            available: MonetaryAmount::new(10.0),
            held: MonetaryAmount::new(0.0),
            history: TransactionHistory::default(),
            is_locked: false,
        };
        let init_ledger: HashMap<ClientId, ClientState> =
            [(client_id, init_state.clone())].into_iter().collect();

        let transactions = vec![
            Transaction::Activity(AccountActivity::Withdrawal(
                client_id,
                TransactionId::new(1),
                MonetaryAmount::new(5.0),
            )),
            Transaction::Dispute(DisputeManagement::Dispute(client_id, TransactionId::new(1))),
        ];

        let final_ledger = create_ledger_with_init(init_ledger, Box::new(transactions.into_iter()));

        let client_ledger = final_ledger
            .0
            .into_iter()
            .find(|x| x.id == client_id)
            .unwrap();

        assert_eq!(client_ledger.total, MonetaryAmount::new(5.0));
        assert_eq!(client_ledger.available, MonetaryAmount::new(5.0));
        assert_eq!(client_ledger.held, MonetaryAmount::new(0.0));
    }

    #[test]
    fn dispute_will_ignore_incorrect_tx() {
        let client_id = ClientId::new(1);

        let init_state = ClientState {
            total: MonetaryAmount::new(10.0),
            available: MonetaryAmount::new(10.0),
            held: MonetaryAmount::new(0.0),
            history: TransactionHistory::default(),
            is_locked: false,
        };
        let init_ledger: HashMap<ClientId, ClientState> =
            [(client_id, init_state.clone())].into_iter().collect();

        let transactions = vec![
            Transaction::Activity(AccountActivity::Deposit(
                client_id,
                TransactionId::new(1),
                MonetaryAmount::new(5.0),
            )),
            Transaction::Dispute(DisputeManagement::Dispute(client_id, TransactionId::new(2))),
        ];

        let final_ledger = create_ledger_with_init(init_ledger, Box::new(transactions.into_iter()));

        let client_ledger = final_ledger
            .0
            .into_iter()
            .find(|x| x.id == client_id)
            .unwrap();

        assert_eq!(client_ledger.total, MonetaryAmount::new(15.0));
        assert_eq!(client_ledger.available, MonetaryAmount::new(15.0));
        assert_eq!(client_ledger.held, MonetaryAmount::new(0.0));
    }

    #[test]
    fn dispute_is_one_per_tx() {
        let client_id = ClientId::new(1);

        let init_state = ClientState {
            total: MonetaryAmount::new(10.0),
            available: MonetaryAmount::new(10.0),
            held: MonetaryAmount::new(0.0),
            history: TransactionHistory::default(),
            is_locked: false,
        };
        let init_ledger: HashMap<ClientId, ClientState> =
            [(client_id, init_state.clone())].into_iter().collect();

        let transactions = vec![
            Transaction::Activity(AccountActivity::Deposit(
                client_id,
                TransactionId::new(1),
                MonetaryAmount::new(5.0),
            )),
            Transaction::Dispute(DisputeManagement::Dispute(client_id, TransactionId::new(1))),
            Transaction::Dispute(DisputeManagement::Dispute(client_id, TransactionId::new(1))),
        ];

        let final_ledger = create_ledger_with_init(init_ledger, Box::new(transactions.into_iter()));

        let client_ledger = final_ledger
            .0
            .into_iter()
            .find(|x| x.id == client_id)
            .unwrap();

        assert_eq!(client_ledger.total, MonetaryAmount::new(15.0));
        assert_eq!(client_ledger.available, MonetaryAmount::new(10.0));
        assert_eq!(client_ledger.held, MonetaryAmount::new(5.0));
    }

    #[test]
    fn resolve_will_release_held_funds() {
        let client_id = ClientId::new(1);

        let init_state = ClientState {
            total: MonetaryAmount::new(10.0),
            available: MonetaryAmount::new(10.0),
            held: MonetaryAmount::new(0.0),
            history: TransactionHistory::default(),
            is_locked: false,
        };
        let init_ledger: HashMap<ClientId, ClientState> =
            [(client_id, init_state.clone())].into_iter().collect();

        let transactions = vec![
            Transaction::Activity(AccountActivity::Deposit(
                client_id,
                TransactionId::new(1),
                MonetaryAmount::new(5.0),
            )),
            Transaction::Dispute(DisputeManagement::Dispute(client_id, TransactionId::new(1))),
            Transaction::Dispute(DisputeManagement::Resolve(client_id, TransactionId::new(1))),
        ];

        let final_ledger = create_ledger_with_init(init_ledger, Box::new(transactions.into_iter()));

        let client_ledger = final_ledger
            .0
            .into_iter()
            .find(|x| x.id == client_id)
            .unwrap();

        assert_eq!(client_ledger.total, MonetaryAmount::new(15.0));
        assert_eq!(client_ledger.available, MonetaryAmount::new(15.0));
        assert_eq!(client_ledger.held, MonetaryAmount::new(0.0));
    }

    #[test]
    fn resolve_against_undisputed_tx_is_ignored() {
        let client_id = ClientId::new(1);

        let init_state = ClientState {
            total: MonetaryAmount::new(10.0),
            available: MonetaryAmount::new(10.0),
            held: MonetaryAmount::new(0.0),
            history: TransactionHistory::default(),
            is_locked: false,
        };
        let init_ledger: HashMap<ClientId, ClientState> =
            [(client_id, init_state.clone())].into_iter().collect();

        let transactions = vec![
            Transaction::Activity(AccountActivity::Deposit(
                client_id,
                TransactionId::new(1),
                MonetaryAmount::new(5.0),
            )),
            Transaction::Dispute(DisputeManagement::Resolve(client_id, TransactionId::new(1))),
        ];

        let final_ledger = create_ledger_with_init(init_ledger, Box::new(transactions.into_iter()));

        let client_ledger = final_ledger
            .0
            .into_iter()
            .find(|x| x.id == client_id)
            .unwrap();

        assert_eq!(client_ledger.total, MonetaryAmount::new(15.0));
        assert_eq!(client_ledger.available, MonetaryAmount::new(15.0));
        assert_eq!(client_ledger.held, MonetaryAmount::new(0.0));
    }

    #[test]
    fn resolve_against_non_tx_is_ignored() {
        let client_id = ClientId::new(1);

        let init_state = ClientState {
            total: MonetaryAmount::new(10.0),
            available: MonetaryAmount::new(10.0),
            held: MonetaryAmount::new(0.0),
            history: TransactionHistory::default(),
            is_locked: false,
        };
        let init_ledger: HashMap<ClientId, ClientState> =
            [(client_id, init_state.clone())].into_iter().collect();

        let transactions = vec![
            Transaction::Activity(AccountActivity::Deposit(
                client_id,
                TransactionId::new(1),
                MonetaryAmount::new(5.0),
            )),
            Transaction::Dispute(DisputeManagement::Dispute(client_id, TransactionId::new(1))),
            Transaction::Dispute(DisputeManagement::Resolve(client_id, TransactionId::new(2))),
        ];

        let final_ledger = create_ledger_with_init(init_ledger, Box::new(transactions.into_iter()));

        let client_ledger = final_ledger
            .0
            .into_iter()
            .find(|x| x.id == client_id)
            .unwrap();

        assert_eq!(client_ledger.total, MonetaryAmount::new(15.0));
        assert_eq!(client_ledger.available, MonetaryAmount::new(10.0));
        assert_eq!(client_ledger.held, MonetaryAmount::new(5.0));
    }

    #[test]
    fn chargeback_locks_account() {
        let client_id = ClientId::new(1);

        let init_state = ClientState {
            total: MonetaryAmount::new(10.0),
            available: MonetaryAmount::new(10.0),
            held: MonetaryAmount::new(0.0),
            history: TransactionHistory::default(),
            is_locked: false,
        };
        let init_ledger: HashMap<ClientId, ClientState> =
            [(client_id, init_state.clone())].into_iter().collect();

        let transactions = vec![
            Transaction::Activity(AccountActivity::Deposit(
                client_id,
                TransactionId::new(1),
                MonetaryAmount::new(5.0),
            )),
            Transaction::Dispute(DisputeManagement::Dispute(client_id, TransactionId::new(1))),
            Transaction::Dispute(DisputeManagement::Chargeback(
                client_id,
                TransactionId::new(1),
            )),
        ];

        let final_ledger = create_ledger_with_init(init_ledger, Box::new(transactions.into_iter()));

        let client_ledger = final_ledger
            .0
            .into_iter()
            .find(|x| x.id == client_id)
            .unwrap();

        assert_eq!(client_ledger.is_locked, true);
    }

    #[test]
    fn chargeback_reduces_total() {
        let client_id = ClientId::new(1);

        let init_state = ClientState {
            total: MonetaryAmount::new(10.0),
            available: MonetaryAmount::new(10.0),
            held: MonetaryAmount::new(0.0),
            history: TransactionHistory::default(),
            is_locked: false,
        };
        let init_ledger: HashMap<ClientId, ClientState> =
            [(client_id, init_state.clone())].into_iter().collect();

        let transactions = vec![
            Transaction::Activity(AccountActivity::Deposit(
                client_id,
                TransactionId::new(1),
                MonetaryAmount::new(5.0),
            )),
            Transaction::Dispute(DisputeManagement::Dispute(client_id, TransactionId::new(1))),
            Transaction::Dispute(DisputeManagement::Chargeback(
                client_id,
                TransactionId::new(1),
            )),
        ];

        let final_ledger = create_ledger_with_init(init_ledger, Box::new(transactions.into_iter()));

        let client_ledger = final_ledger
            .0
            .into_iter()
            .find(|x| x.id == client_id)
            .unwrap();

        assert_eq!(client_ledger.total, MonetaryAmount::new(10.));
    }

    #[test]
    fn chargeback_reduces_held() {
        let client_id = ClientId::new(1);

        let init_state = ClientState {
            total: MonetaryAmount::new(10.0),
            available: MonetaryAmount::new(10.0),
            held: MonetaryAmount::new(0.0),
            history: TransactionHistory::default(),
            is_locked: false,
        };
        let init_ledger: HashMap<ClientId, ClientState> =
            [(client_id, init_state.clone())].into_iter().collect();

        let transactions = vec![
            Transaction::Activity(AccountActivity::Deposit(
                client_id,
                TransactionId::new(1),
                MonetaryAmount::new(5.0),
            )),
            Transaction::Dispute(DisputeManagement::Dispute(client_id, TransactionId::new(1))),
            Transaction::Dispute(DisputeManagement::Chargeback(
                client_id,
                TransactionId::new(1),
            )),
        ];

        let final_ledger = create_ledger_with_init(init_ledger, Box::new(transactions.into_iter()));

        let client_ledger = final_ledger
            .0
            .into_iter()
            .find(|x| x.id == client_id)
            .unwrap();

        assert_eq!(client_ledger.held, MonetaryAmount::new(0.));
    }

    #[test]
    fn chargeback_ignored_if_tx_does_not_exist() {
        let client_id = ClientId::new(1);

        let init_state = ClientState {
            total: MonetaryAmount::new(10.0),
            available: MonetaryAmount::new(10.0),
            held: MonetaryAmount::new(0.0),
            history: TransactionHistory::default(),
            is_locked: false,
        };
        let init_ledger: HashMap<ClientId, ClientState> =
            [(client_id, init_state.clone())].into_iter().collect();

        let transactions = vec![
            Transaction::Activity(AccountActivity::Deposit(
                client_id,
                TransactionId::new(1),
                MonetaryAmount::new(5.0),
            )),
            Transaction::Dispute(DisputeManagement::Dispute(client_id, TransactionId::new(1))),
            Transaction::Dispute(DisputeManagement::Chargeback(
                client_id,
                TransactionId::new(2),
            )),
        ];

        let final_ledger = create_ledger_with_init(init_ledger, Box::new(transactions.into_iter()));

        let client_ledger = final_ledger
            .0
            .into_iter()
            .find(|x| x.id == client_id)
            .unwrap();

        assert_eq!(client_ledger.total, MonetaryAmount::new(15.));
        assert_eq!(client_ledger.available, MonetaryAmount::new(10.));
        assert_eq!(client_ledger.held, MonetaryAmount::new(5.));
    }

    #[test]
    fn chargeback_ignored_if_tx_undisputed() {
        let client_id = ClientId::new(1);

        let init_state = ClientState {
            total: MonetaryAmount::new(10.0),
            available: MonetaryAmount::new(10.0),
            held: MonetaryAmount::new(0.0),
            history: TransactionHistory::default(),
            is_locked: false,
        };
        let init_ledger: HashMap<ClientId, ClientState> =
            [(client_id, init_state.clone())].into_iter().collect();

        let transactions = vec![
            Transaction::Activity(AccountActivity::Deposit(
                client_id,
                TransactionId::new(1),
                MonetaryAmount::new(5.0),
            )),
            Transaction::Dispute(DisputeManagement::Chargeback(
                client_id,
                TransactionId::new(1),
            )),
        ];

        let final_ledger = create_ledger_with_init(init_ledger, Box::new(transactions.into_iter()));

        let client_ledger = final_ledger
            .0
            .into_iter()
            .find(|x| x.id == client_id)
            .unwrap();

        assert_eq!(client_ledger.total, MonetaryAmount::new(15.));
        assert_eq!(client_ledger.available, MonetaryAmount::new(15.));
        assert_eq!(client_ledger.held, MonetaryAmount::new(0.));
    }
}
