use std::collections::HashSet;

use reqwest::StatusCode;
use serde_json::Value;

pub async fn get_countries() -> (HashSet<String>, Vec<String>) {
    let a = vec!["Denmark".to_string()];
    return (a.clone().into_iter().collect(), a);

    let response = reqwest::get("https://restcountries.com/v3.1/all/?fields=name")
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    if let Value::Array(values) = response.json().await.unwrap() {
        let countries = values
            .into_iter()
            .map(|v| {
                v.as_object()
                    .unwrap()
                    .get("name")
                    .unwrap()
                    .as_object()
                    .unwrap()
                    .get("common")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect::<Vec<String>>();
        (countries.iter().cloned().collect(), countries)
    } else {
        panic!("")
    }
}
