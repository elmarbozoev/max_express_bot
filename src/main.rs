use bot::BotService;

mod models;
mod vendor;
mod database;
mod bot;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    env_logger::init();

    log::info!("Starting max_express_bot");

    dotenv::dotenv().ok();

    let bot = BotService::new().await;

    bot.dispatch().await;

    Ok(())
}
