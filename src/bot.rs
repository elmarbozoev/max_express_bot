use std::{default, env};
use dptree::di;
use indoc::indoc;
use teloxide::{dispatching::{dialogue::{self, Dialogue, GetChatId, InMemStorage}, Dispatcher, HandlerExt, UpdateFilterExt}, payloads::{EditMessageTextSetters, SendMessageSetters}, requests::Requester, types::{CallbackQuery, InlineKeyboardButton, InlineKeyboardMarkup, Message, MessageId, Update}, Bot};

use crate::{database::Db, models::User, vendor::product_ready};

pub struct BotService {
    bot: Bot,
    db: Db
}

#[derive(Clone, Default)]
enum BotState {
    #[default]
    Start,
    SignIn,
    Main,
    ProductStatus
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

#[derive(Clone, Default, PartialEq)]
enum MainState {
    #[default]
    Init,
    Profile {
        msg_id: MessageId
    },
    Pages {
        msg_id: MessageId
    }
}

type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

type BotDialogue = Dialogue<BotState, InMemStorage<BotState>>;
type SignInDialogue = Dialogue<SignInState, InMemStorage<SignInState>>;
type MainDialogue = Dialogue<MainState, InMemStorage<MainState>>;

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

        let message_handler = Update::filter_message()
            .branch(dptree::case![BotState::Start].endpoint(Self::start))
            .branch(dptree::case![BotState::SignIn]
                .enter_dialogue::<Message, InMemStorage<SignInState>, SignInState>()
                .branch(dptree::case![SignInState::Start].endpoint(Self::start_signing_in))
                .branch(dptree::case![SignInState::FirstName].endpoint(Self::sign_first_name_in))
                .branch(dptree::case![SignInState::LastName { reserved_first_name }].endpoint(Self::sign_last_name_in))
                .branch(dptree::case![SignInState::PhoneNumber { reserved_first_name, reserved_last_name }].endpoint(Self::sign_phone_number_in))
            )
            .branch(dptree::case![BotState::ProductStatus].endpoint(Self::get_product_status));

        let callback_handler = Update::filter_callback_query()
            .branch(dptree::case![BotState::Main]
                .enter_dialogue::<CallbackQuery, InMemStorage<MainState>, MainState>()
                .branch(dptree::case![MainState::Init].endpoint(Self::send_profile))
                .branch(dptree::case![MainState::Profile { msg_id }].endpoint(Self::send_profile))
                .branch(dptree::case![MainState::Pages { msg_id }].endpoint(Self::handle_pages))
            );


        let handler = dialogue::enter::<Update, InMemStorage<BotState>, BotState, _>()
            .branch(message_handler)
            .branch(callback_handler);

        

        Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![
                InMemStorage::<BotState>::new(),
                InMemStorage::<SignInState>::new(),
                InMemStorage::<MainState>::new(),
                self.db.clone()])
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;
    }

    async fn start(bot: Bot, dialogue: BotDialogue, msg: Message, db: Db) -> HandlerResult {
        let user_id = msg.from().expect("ERROR: user is unknown").id.0 as i64;
        
        if db.check_user(user_id).await {
            dialogue.update(BotState::Main).await.expect("ERROR: Start -> Main");

            let markup = InlineKeyboardMarkup::new(
                vec![vec![InlineKeyboardButton::callback("Продолжить", "continue_btn")]]
            );

            bot.send_message(msg.chat.id, "С возвращением!").reply_markup(markup)
                .await?;

            return Ok(());
        }

        bot.send_message(msg.chat.id, indoc!(r#"
        Добро пожаловать в MaxExpress! 😊
                        
        У нас Вы можете:
                        
        1) Отслеживать статус доставки 🚚
        2) Получить свой клиентский код 💼
        3) Узнать способы оплаты 💳 (по весу или по плотности)

        Напишите что-нибудь, чтобы начать.
        "#)).await.expect("ERROR: Could not send a message (start)");
        
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
                bot.send_message(msg.chat.id, indoc!(r#"
                Неверный формат.
                Введите имя еще раз.
                "#)).await.expect("ERROR: Could not send a message (sign_in_first_name)");

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
                bot.send_message(msg.chat.id, indoc!(r#"
                Неверный формат.
                Введите фамилию еще раз.
                "#)).await.expect("ERROR: Could not send a message (sign_last_name_in)");

                dialogue.update(SignInState::LastName { reserved_first_name: first_name })
                    .await
                    .expect("ERROR: LastName -> LastName");

                return Ok(());
            }
        };

        bot.send_message(msg.chat.id, indoc!(r#"
        Напишите Ваш номер телефона
        Пример: 996XXXXXXXXX.
        "#)).await.expect("ERROR: Could not send a message in sign_last_name_in");

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
                bot.send_message(msg.chat.id, indoc!(r#"
                Неверный формат.
                Введите номер телефона еще раз.
                Пример: 996XXXXXXXXX
                "#)).await.expect("ERROR: Could not send a message (sign_phone_number_in)");

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

        bot_dialogue.update(BotState::Main)
            .await
            .expect("ERROR: SignIn -> Main");

        Ok(())
    }

    async fn send_profile(bot: Bot, dialogue: MainDialogue, q: CallbackQuery, db: Db) -> HandlerResult {
        let msg = q.message.unwrap();
        let telegram_id = q.from.id.0 as i64;
        let user = db.get_user(telegram_id).await;

        let message = format!(
        indoc!(r#"
        Ваш профиль:
                        
        📃 Клиентский код: {}
        👤 Имя: {}
        👤 Фамилия: {}
        📞 Номер тел: {}
        "#), &user.client_code, &user.first_name, &user.last_name, &user.phone_number);

        let markup = InlineKeyboardMarkup::new(
            vec![
                vec![InlineKeyboardButton::callback("Отслеживание ", "locate_btn")],
                vec![
                    InlineKeyboardButton::callback("Код", "code_btn"),
                    InlineKeyboardButton::callback("Адрес", "address_btn")
                ],
                vec![
                    InlineKeyboardButton::callback("Тех. поддержка", "service_btn"),
                    InlineKeyboardButton::callback("Инструкция", "tutorial_btn")
                ]
            ]
        );

        let msg_id = match dialogue.get_or_default().await? {
            MainState::Init => bot.send_message(msg.chat.id, message).reply_markup(markup).await?.id,
            MainState::Pages { msg_id } | MainState::Profile { msg_id } => bot.edit_message_text(msg.chat.id, msg_id, message).reply_markup(markup).await?.id
        };

        dialogue.update(MainState::Pages { msg_id })
            .await
            .expect("ERROR: Regular -> Regular");

        Ok(())
    }

    async fn handle_pages(
            bot: Bot, 
            dialogue: MainDialogue,
            bot_dialogue: BotDialogue,
            q: CallbackQuery) -> HandlerResult {
        let msg_id: Option<MessageId> = if let MainState::Pages{ msg_id} = dialogue.get().await?.unwrap() {
            Some(msg_id)
        } else {
            None
        };
        
        if q.data == None {
            dialogue.update(MainState::Profile { msg_id: msg_id.unwrap() }).await?;
            return Ok(());
        }

        let cb = q.data.unwrap();

        let markup = InlineKeyboardMarkup::new(vec![
            vec![InlineKeyboardButton::callback("Назад", "back_btn")]
        ]);

        let message = match cb.as_str() {
            "locate_btn" => {
                bot_dialogue.update(BotState::ProductStatus).await?;
                "Введите трек-код товара"
            },
            "code_btn" => "Код",
            "address_btn" => "Адрес",
            "service_btn" => "Тех поддержка",
            "tutorial_btn" => "Инструкция",
            _ => " "
        };
        
        bot.edit_message_text(q.message.unwrap().chat.id, msg_id.unwrap(), message).reply_markup(markup).await?.id;
        
        dialogue.update(MainState::Profile { msg_id: msg_id.unwrap() }).await?;

        Ok(())
    }

    async fn get_product_status(bot: Bot, bot_dialogue: BotDialogue, main_dialogue: MainDialogue, msg: Message) -> HandlerResult {
        let ready = product_ready(msg.text().unwrap()).await;

        let message = if ready {
            "Товар уже на складе, ждет сортировки"
        } else {
            "Товар еще не на складе"
        };

        let markup = InlineKeyboardMarkup::new(
            vec![vec![InlineKeyboardButton::callback("Вернуться в личный кабинет", "back_btn")]]
        );

        bot.send_message(msg.chat.id, message).reply_markup(markup).await?;

        bot_dialogue.update(BotState::Main).await?;
        main_dialogue.update(MainState::Init).await?;

        Ok(())
    }
}
