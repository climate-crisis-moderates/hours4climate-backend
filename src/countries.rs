use std::collections::HashMap;

#[allow(non_camel_case_types)]
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub enum Origin {
    world,
    country,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct Country {
    pub id: String,
    pub name: String,
    pub origin: Origin,
    pub emissions_year: u32,
    pub emissions_unit: String,
    pub emissions: i64,
    pub employees_year: u32,
    pub employees: u64,
    pub employees_unit: String,
}

pub async fn get_countries() -> HashMap<String, Country> {
    let file = std::fs::File::open("countries.json").expect("file should open read only");

    serde_json::from_reader(file).unwrap()
}
