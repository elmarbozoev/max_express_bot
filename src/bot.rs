use std::env;
use teloxide::{dispatching::{dialogue::{Dialogue, InMemStorage}, Dispatcher, HandlerExt, UpdateFilterExt}, payloads::SendMessageSetters, requests::Requester, types::{CallbackQuery, InlineKeyboardButton, InlineKeyboardMarkup, Message, Update}, Bot};

use crate::{database::Db, models::User};

pub struct BotService {
    bot: Bot,
    db: Db
}

#[derive(Clone, Default)]
pub enum BotState {
    #[default]
    Start,
    SignIn,
    Regular
}

#[derive(Clone, Default)]
enum SignInState {
    #[default]
    Start,
    FirstName,
    LastName {
        reserved_first_name: String
    },
    PhoneNumber {
        reserved_first_name: String,
        reserved_last_name: String
    }
}

type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;
type BotDialogue = Dialogue<BotState, InMemStorage<BotState>>;
type SignInDialogue = Dialogue<SignInState, InMemStorage<SignInState>>;

impl BotService {
    pub async fn new(db_url: &str) -> BotService {
        BotService {
            bot: Bot::from_env(),
            db: Db::new(db_url).await
        }
    }

    pub async fn from_env() -> BotService {
        let url = env::var("DATABASE_URL")
            .expect("ERROR: Could not get db url from env");
        Self::new(url.as_str()).await
    }

    pub async fn dispatch(&self) {
        let bot = self.bot.clone();

        let handler = Update::filter_message()
            .enter_dialogue::<Message, InMemStorage<BotState>, BotState>()
            .branch(dptree::case![BotState::Start].endpoint(Self::start))
            .branch(dptree::case![BotState::SignIn]
                .enter_dialogue::<Message, InMemStorage<SignInState>, SignInState>()
                .branch(dptree::case![SignInState::Start].endpoint(Self::start_signing_in))
                .branch(dptree::case![SignInState::FirstName].endpoint(Self::sign_first_name_in))
                .branch(dptree::case![SignInState::LastName { reserved_first_name }].endpoint(Self::sign_last_name_in))
                .branch(dptree::case![SignInState::PhoneNumber { reserved_first_name, reserved_last_name }].endpoint(Self::sign_phone_number_in))
            )
            .branch(dptree::case![BotState::Regular].endpoint(Self::listen_regular));

        Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![
                InMemStorage::<BotState>::new(),
                InMemStorage::<SignInState>::new(),
                self.db.clone()])
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;
    }

    async fn start(bot: Bot, dialogue: BotDialogue, msg: Message, db: Db) -> HandlerResult {
        let user_id = msg.from().expect("ERROR: user is unknown").id.0 as i64;
        
        if db.check_user(user_id).await {
            dialogue.update(BotState::Regular).await.expect("ERROR: Start -> Regular");
            return Ok(());
        }

        let markup = InlineKeyboardMarkup::new(
            vec![vec![InlineKeyboardButton::callback("Начать", "start_btn")]]
        );

        bot.send_message(msg.chat.id, r#"
        Добро пожаловать в MaxExpress! 😊
                  
        У нас Вы можете:
                  
        1) Отслеживать статус доставки 🚚
        2) Получить свой клиентский код 💼
        3) Узнать способы оплаты 💳 (по весу или по плотности)
        "#).reply_markup(markup).await.expect("ERROR: Could not send a message (start)");

        dialogue.update(BotState::SignIn).await.expect("ERROR: Start -> Registration");

        Ok(())
    }

    async fn start_signing_in(bot: Bot, dialogue: SignInDialogue, msg: Message) -> HandlerResult {
        bot.send_message(msg.chat.id, r#"
        Пройдите быструю и легкую регистрацию, чтобы получить свой клиентский код!
        "#).await.expect("ERROR: Could not send a message (start_signing_in)");

        bot.send_message(msg.chat.id, r#"
        Напишите Ваше имя.
        "#).await.expect("ERROR: Could not send a message (start_signing_in)");
        
        dialogue.update(SignInState::FirstName)
            .await
            .expect("ERROR: Start -> FirstName");

        Ok(())
    }

    async fn sign_first_name_in(bot: Bot, dialogue: SignInDialogue, msg: Message) -> HandlerResult {
        let mut first_name = String::new();
        
        match msg.text() {
            Some(name) => {
                first_name = name.to_string();
            },
            None => {
                bot.send_message(msg.chat.id, r#"
                Неверный формат.
                Введите имя еще раз.
                "#).await.expect("ERROR: Could not send a message (sign_in_first_name)");

                dialogue.update(SignInState::FirstName)
                    .await
                    .expect("ERROR: FirstName -> FirstName");

                return Ok(());
            }
        };

        bot.send_message(msg.chat.id, r#"
        Напишите Вашу фамилию.
        "#).await.expect("ERROR: Could not send a message in sign_first_name_in");

        dialogue.update(SignInState::LastName { reserved_first_name: first_name })
            .await
            .expect("ERROR: FirstName -> LastName");

        Ok(())
    }

    async fn sign_last_name_in(bot: Bot, dialogue: SignInDialogue, msg: Message) -> HandlerResult {
        let mut last_name = String::new();
        let first_name = match dialogue.get()
            .await
            .expect("ERROR: Could not get first name from SignInState")
            .expect("ERROR: SignInState have not first name") {
                SignInState::LastName { reserved_first_name } => reserved_first_name,
                _ => "".to_string()
        };
        
        match msg.text() {
            Some(name) => {
                last_name = name.to_string();
            },
            None => {
                bot.send_message(msg.chat.id, r#"
                Неверный формат.
                Введите фамилию еще раз.
                "#).await.expect("ERROR: Could not send a message (sign_last_name_in)");

                dialogue.update(SignInState::LastName { reserved_first_name: first_name })
                    .await
                    .expect("ERROR: LastName -> LastName");

                return Ok(());
            }
        };

        bot.send_message(msg.chat.id, r#"
        Напишите Ваш номер телефона
        Пример: 996XXXXXXXXX.
        "#).await.expect("ERROR: Could not send a message in sign_last_name_in");

        dialogue.update(SignInState::PhoneNumber {reserved_first_name: first_name, reserved_last_name: last_name})
            .await
            .expect("ERROR: FirstName -> LastName");

        Ok(())
    }

    async fn sign_phone_number_in(
            bot: Bot,
            dialogue: SignInDialogue,
            bot_dialogue: BotDialogue,
            msg: Message,
            db: Db) -> HandlerResult {
        
        let mut phone_number = String::new();
        let (first_name, last_name) = match dialogue.get()
            .await
            .expect("ERROR: Could not get full name from SignInState")
            .expect("ERROR: SignInState have not full name") {
                SignInState::PhoneNumber { reserved_first_name, reserved_last_name } 
                    => (reserved_first_name, reserved_last_name),
                _ => ("".to_string(), "".to_string())
        };
        let telegram_id = msg.from().expect("ERROR: user is unknown").id.0 as i64;

        match msg.text() {
            Some(phone) => {
                phone_number = phone.to_string();
            },
            None => {
                bot.send_message(msg.chat.id, r#"
                Неверный формат.
                Введите номер телефона еще раз.
                Пример: 996XXXXXXXXX
                "#).await.expect("ERROR: Could not send a message (sign_phone_number_in)");

                dialogue.update(SignInState::PhoneNumber { 
                    reserved_first_name: first_name, 
                    reserved_last_name: last_name })
                    .await
                    .expect("ERROR: PhoneNumber -> PhoneNumber");

                return Ok(());
            }
        };

        let user = User {
            id: 0,
            client_code: String::new(),
            first_name,
            last_name,
            phone_number,
            telegram_id
        };

        db.create_user(user).await;

        let markup = InlineKeyboardMarkup::new(
            vec![vec![InlineKeyboardButton::callback("Далее", "next")]]
        );

        bot.send_message(msg.chat.id, "Вы зарегистрированы!")
            .reply_markup(markup)
            .await
            .expect("ERROR: Could not send a message in sign_phone_number_in");

        dialogue.exit()
            .await
            .expect("ERROR: Could not exit SignInDialogue");

        bot_dialogue.update(BotState::Regular)
            .await
            .expect("ERROR: SignIn -> Regular");

        Ok(())
    }

    async fn listen_regular(bot: Bot, dialogue: BotDialogue, msg: Message) -> HandlerResult {
        bot.send_message(msg.chat.id, "DEVELOPMENT!")
            .await
            .expect("ERROR: Could not send a message in listen_regular");

        dialogue.update(BotState::Regular)
            .await
            .expect("ERROR: Regular -> Regular");

        Ok(())
    }
}
