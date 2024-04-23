use serde::Deserialize;
use sqlx::prelude::FromRow;

#[derive(FromRow, Clone)]
pub struct User {
    pub id: i32,
    pub first_name: String,
    pub last_name: String,
    pub phone_number: String,
    pub telegram_id: i64,
    pub client_code: String
}

impl User {
    pub fn new() -> User {
        User {
            id: 0,
            first_name: String::new(),
            last_name: String::new(),
            phone_number: String::new(),
            telegram_id: 0_i64,
            client_code: String::new()
        }
    }
}

#[derive(Deserialize)]
pub struct ProductStatus {
    pub code: String,
    pub msg: String
}