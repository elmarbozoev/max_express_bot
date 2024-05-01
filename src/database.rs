use sqlx::{query_as, query_scalar, PgPool};

use sqlx::{query, Pool, Postgres};
use crate::models::User;

#[derive(Clone)]
pub struct Db {
    pool: Pool<Postgres>
}

impl Db {
    pub async fn new(url: &str) -> Db {
        Db {
            pool: PgPool::connect(url).await.expect("ERROR: Could not connect the database")
        }
    }

    pub async fn create_user(&self, mut new_user: User) {
        let count: i64 = query_scalar!("SELECT COUNT(*) AS user_count FROM users;")
            .fetch_all(&self.pool)
            .await.expect("ERROR: Could not get user count")[0].unwrap();

        let client_code: String = "MX".to_string() + &(200 + count).to_string();

        new_user.client_code = client_code;

        query("INSERT INTO users (first_name, last_name, phone_number, telegram_id, client_code)
            VALUES ($1, $2, $3, $4, $5);")
            .bind(new_user.first_name)
            .bind(new_user.last_name)
            .bind(new_user.phone_number)
            .bind(new_user.telegram_id)
            .bind(new_user.client_code)
            .fetch_all(&self.pool)
            .await.expect("ERROR: Could not create a user");
    }

    pub async fn get_user(&self, telegram_id: i64) -> User {
        query_as::<_, User>("SELECT * FROM users WHERE telegram_id = $1;")
            .bind(telegram_id)
            .fetch_all(&self.pool)
            .await.expect("ERROR: Could not get user")[0].clone()
    }

    pub async fn check_user(&self, telegram_id: i64) -> bool {
        query_scalar!("SELECT EXISTS (SELECT 1 FROM users WHERE telegram_id = $1);", telegram_id)
            .fetch_all(&self.pool)
            .await.expect("ERROR: Could check the user")[0].expect("ERROR: Could not check the user")
    }
}