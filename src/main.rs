use std::env;
use dotenv::dotenv;
use sqlx::PgPool;

mod models;
mod vendor;
mod database;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("ERROR: Couldn't find DATABASE_URL");
    let pool = PgPool::connect(database_url.as_str()).await?;

    Ok(())
}
