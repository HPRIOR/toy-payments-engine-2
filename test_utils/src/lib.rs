use std::{collections::HashMap, str};

use serde::Serialize;

#[derive(Serialize)]
struct ClientRow {
    client: &'static str,
    available: &'static str,
    held: &'static str,
    total: &'static str,
    locked: &'static str,
}

impl ClientRow {
    fn new(
        client: &'static str,
        available: &'static str,
        held: &'static str,
        total: &'static str,
        locked: &'static str,
    ) -> Self {
        Self {
            client,
            available,
            held,
            total,
            locked,
        }
    }
}

// Only used during testing so no need to return result
pub fn create_csv(rows: Vec<[&'static str; 5]>) -> String {
    let client_rows: Vec<ClientRow> = rows
        .into_iter()
        .map(|r| ClientRow::new(r[0], r[1], r[2], r[3], r[4]))
        .collect();

    let mut wtr = csv::Writer::from_writer(vec![]);
    for c in client_rows {
        wtr.serialize(c).unwrap();
    }
    wtr.flush().unwrap();
    let data = String::from_utf8(wtr.into_inner().unwrap()).unwrap();
    data
}

fn split_to_dict(csv: &String) -> HashMap<String, String> {
    csv.split("\n")
        .skip(1) // ignore row titles
        .into_iter()
        .map(|line| {
            (
                line.split(",").nth(0).unwrap().to_string(),
                line.to_string(),
            )
        })
        .collect()
}

// Ordering of csv is not guaranteed during integration testing. This is used to ensure tests are
// not flaky
pub fn assert_unsorted_eq(s1: &String, s2: &String){
    let sut1 = split_to_dict(s1);
    let sut2 = split_to_dict(s2);
    if sut1.len() != sut2.len(){
        panic!("csvs do not contain the same number of rows");
    }

    sut1.iter().for_each(|(k, v)| {
        let maybe_sut2_row = sut2.get(k);
        match maybe_sut2_row {
            Some(row) => {assert_eq!(row, v)},
            None => {panic!("client {} not found in both csvs", k)},
        }
    })

}

#[cfg(test)]
mod tests {
    use crate::{create_csv, split_to_dict, assert_unsorted_eq};

    #[test]
    fn create_csv_creates_single_row() {
        let rows = vec![["1", "2", "3", "4", "5"]];
        let sut = create_csv(rows);
        let expected = String::from("client,available,held,total,locked\n1,2,3,4,5\n");
        assert_eq!(sut, expected);
    }

    #[test]
    fn create_csv_creates_multiple_rows() {
        let rows = vec![["1", "2", "3", "4", "5"], ["1", "2", "3", "4", "5"]];
        let sut = create_csv(rows);
        let expected = String::from("client,available,held,total,locked\n1,2,3,4,5\n1,2,3,4,5\n");
        assert_eq!(sut, expected);
    }

    #[test]
    fn csvs_are_split_into_dicts() {
        let csv = String::from("client,available,held,total,locked\n1,2,3,4,5\n2,2,3,4,5\n");
        let sut = split_to_dict(&csv);
        assert_eq!(sut["1"], String::from("1,2,3,4,5"));
        assert_eq!(sut["2"], String::from("2,2,3,4,5"));
    }


    #[test]
    fn two_unsorted_csvs_will_assert_eq(){
        let csv1 = String::from("client,available,held,total,locked\n1,2,3,4,5\n2,2,3,4,5\n3,2,3,4,5\n");
        let csv2 = String::from("client,available,held,total,locked\n2,2,3,4,5\n3,2,3,4,5\n1,2,3,4,5\n");
        assert_unsorted_eq(&csv1, &csv2);
    }


    #[test]
    #[should_panic]
    fn two_unequal_len_csvs_will_assert_false(){
        let csv1 = String::from("client,available,held,total,locked\n1,2,3,4,5\n2,2,3,4,5");
        let csv2 = String::from("client,available,held,total,locked\n2,2,3,4,5\n3,2,3,4,5\n1,2,3,4,5\n");
        assert_unsorted_eq(&csv1, &csv2);
    }

    #[test]
    #[should_panic]
    fn two_unequal_csvs_will_assert_false(){
        let csv1 = String::from("client,available,held,total,locked\n1,3,3,4,5\n2,2,3,4,5\n3,2,3,4,5\n");
        let csv2 = String::from("client,available,held,total,locked\n2,2,3,4,5\n3,2,3,4,5\n1,2,3,4,5\n");
        assert_unsorted_eq(&csv1, &csv2);
    }
}

