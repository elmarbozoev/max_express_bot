use bot::BotService;
use dotenv::dotenv;

mod models;
mod vendor;
mod database;
mod bot;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    dotenv().ok();

    let bot = BotService::from_env().await;

    bot.dispatch().await;

    Ok(())
}
