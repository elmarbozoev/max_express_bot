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

#[derive(Deserialize)]
pub struct ProductStatus {
    pub code: String,
    pub msg: String
}