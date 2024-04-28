use std::{env, fs::File, io::Read, path::Path};
use indoc::indoc;
use teloxide::{dispatching::{dialogue::{self, Dialogue, GetChatId, InMemStorage}, Dispatcher, UpdateFilterExt}, payloads::{EditMessageTextSetters, SendDocumentSetters, SendMessageSetters}, requests::Requester, types::{CallbackQuery, ChatId, InlineKeyboardButton, InlineKeyboardMarkup, InputFile, Message, MessageId, Update}, Bot};

use crate::{database::Db, models::User, vendor::product_ready};

pub struct BotService {
    bot: Bot,
    db: Db
}

#[derive(Clone, Default)]
enum BotState {
    #[default]
    Start,
    RegisterInit,
    RegisterFirstName,
    RegisterLastName {
        first_name: String
    },
    RegisterPhoneNumber {
        first_name: String,
        last_name: String
    },
    Profile {
        msg_id: MessageId
    },
    ProfilePages {
        msg_id: MessageId
    },
    ProductStatus {
        msg_id: MessageId
    },
    Tutorial {
        msg_id: MessageId
    },
    PriceWidth,
    PriceLength {
        width: f32
    },
    PriceHeight {
        width: f32,
        length: f32
    },
    PriceWeight {
        width: f32,
        length: f32,
        height: f32
    }
}

type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

type BotDialogue = Dialogue<BotState, InMemStorage<BotState>>;

impl BotService {
    pub async fn new(db_url: &str) -> BotService {
        let bot_service = BotService {
            bot: Bot::from_env(),
            db: Db::new(db_url).await
        };

        bot_service.db.init_table().await.expect("ERROR: Could init table");

        bot_service
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
            .branch(dptree::case![BotState::RegisterFirstName].endpoint(Self::register_first_name))
            .branch(dptree::case![BotState::RegisterLastName { first_name }].endpoint(Self::register_last_name))
            .branch(dptree::case![BotState::RegisterPhoneNumber { first_name, last_name }].endpoint(Self::register_phone_number))
            .branch(dptree::case![BotState::ProductStatus { msg_id }].endpoint(Self::get_product_status))
            .branch(dptree::case![BotState::PriceWidth].endpoint(Self::receive_width))
            .branch(dptree::case![BotState::PriceLength { width }].endpoint(Self::receive_length))
            .branch(dptree::case![BotState::PriceHeight { width, length }].endpoint(Self::receive_height))
            .branch(dptree::case![BotState::PriceWeight { width, length, height }].endpoint(Self::receive_weight));

        let callback_handler = Update::filter_callback_query()
            .branch(dptree::case![BotState::RegisterInit].endpoint(Self::init_register)) 
            .branch(dptree::case![BotState::Profile { msg_id }].endpoint(Self::send_profile))
            .branch(dptree::case![BotState::ProductStatus { msg_id }].endpoint(Self::send_profile))
            .branch(dptree::case![BotState::ProfilePages { msg_id }].endpoint(Self::handle_pages))
            .branch(dptree::case![BotState::Tutorial { msg_id }].endpoint(Self::handle_tutorials));


        let handler = dialogue::enter::<Update, InMemStorage<BotState>, BotState, _>()
            .branch(message_handler)
            .branch(callback_handler);

        

        Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![
                InMemStorage::<BotState>::new(),
                self.db.clone()])
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;
    }

    async fn start(bot: Bot, dialogue: BotDialogue, msg: Message, db: Db) -> HandlerResult {
        let user_id = msg.from().expect("ERROR: user is unknown").id.0 as i64;
        
        if db.check_user(user_id).await {
            let markup = InlineKeyboardMarkup::new(
                vec![vec![InlineKeyboardButton::callback("Продолжить", "continue_btn")]]
            );

            let msg_id = bot.send_message(msg.chat.id, "С возвращением!").reply_markup(markup)
                .await?.id;

            dialogue.update(BotState::Profile { msg_id }).await?;

            return Ok(());
        }

        let markup = InlineKeyboardMarkup::new(
            vec![vec![InlineKeyboardButton::callback("Начать", "start_btn")]]
        );

        bot.send_message(msg.chat.id, indoc!(r#"
        Добро пожаловать в MaxExpress! 😊
                        
        У нас Вы можете:
                        
        1) Отслеживать статус доставки 🚚
        2) Получить свой клиентский код 💼
        3) Узнать способы оплаты 💳 (по весу или по плотности)
        "#)).reply_markup(markup).await?;
        
        dialogue.update(BotState::RegisterInit).await?;

        Ok(())
    }

    async fn init_register(bot: Bot, dialogue: BotDialogue, q: CallbackQuery) -> HandlerResult {
        let chat_id = q.chat_id().unwrap();

        bot.send_message(chat_id, r#"
        Пройдите быструю и легкую регистрацию, чтобы получить свой клиентский код!
        "#).await?;

        bot.send_message(chat_id, r#"
        Напишите Ваше имя.
        "#).await?;
        
        dialogue.update(BotState::RegisterFirstName).await?;

        Ok(())
    }

    async fn register_first_name(bot: Bot, dialogue: BotDialogue, msg: Message) -> HandlerResult {
        let mut first_name = String::new();
        
        match msg.text() {
            Some(text) => {
                first_name = text.to_string();
            },
            None => {
                bot.send_message(msg.chat.id, indoc!(r#"
                Неверный формат.
                Введите имя еще раз.
                "#)).await?;

                dialogue.update(BotState::RegisterFirstName)
                    .await?;

                return Ok(());
            }
        };

        bot.send_message(msg.chat.id, r#"
        Напишите Вашу фамилию.
        "#).await?;

        dialogue.update(BotState::RegisterLastName { first_name }).await?;

        Ok(())
    }

    async fn register_last_name(bot: Bot, dialogue: BotDialogue, msg: Message) -> HandlerResult {
        let mut last_name = String::new();

        let first_name = match dialogue.get()
            .await?
            .expect("ERROR: SignInState have not first name") {
                BotState::RegisterLastName { first_name } => first_name,
                _ => "".to_string()
        };
        
        match msg.text() {
            Some(text) => {
                last_name = text.to_string();
            },
            None => {
                bot.send_message(msg.chat.id, indoc!(r#"
                Неверный формат.
                Введите фамилию еще раз.
                "#)).await?;

                dialogue.update(BotState::RegisterLastName { first_name }).await?;

                return Ok(());
            }
        };

        bot.send_message(msg.chat.id, indoc!(r#"
        Напишите Ваш номер телефона
        Пример: 996XXXXXXXXX.
        "#)).await?;

        dialogue.update(BotState::RegisterPhoneNumber { first_name, last_name }).await?;

        Ok(())
    }

    async fn register_phone_number(bot: Bot, dialogue: BotDialogue, msg: Message, db: Db) -> HandlerResult {
        let mut phone_number = String::new();

        let (first_name, last_name) = match dialogue.get()
            .await?.unwrap() {
                BotState::RegisterPhoneNumber { first_name, last_name }
                    => (first_name, last_name),
                _ => ("".to_string(), "".to_string())
        };

        let telegram_id = msg.from().expect("ERROR: user is unknown").id.0 as i64;

        match msg.text() {
            Some(text) => {
                phone_number = text.to_string();
            },
            None => {
                bot.send_message(msg.chat.id, indoc!(r#"
                Неверный формат.
                Введите номер телефона еще раз.
                Пример: 996XXXXXXXXX
                "#)).await?;

                dialogue.update(BotState::RegisterPhoneNumber { first_name, last_name }).await?;

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

        let msg_id = bot.send_message(msg.chat.id, "Вы зарегистрированы!")
            .reply_markup(markup)
            .await?.id;

        dialogue.update(BotState::Profile { msg_id }).await?;

        Ok(())
    }

    async fn send_profile(bot: Bot, dialogue: BotDialogue, q: CallbackQuery, db: Db) -> HandlerResult {
        let chat_id = q.chat_id().unwrap();
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
                vec![InlineKeyboardButton::callback("Отслеживание товара", "locate_btn")],
                vec![InlineKeyboardButton::callback("Высчитывание цены", "price_btn")],
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

        let mut msg_id = match dialogue.get().await?.unwrap() {
            BotState::Profile { msg_id } => msg_id,
            _ => MessageId(0)
        };

        msg_id = bot.edit_message_text(chat_id, msg_id, message).reply_markup(markup).await?.id;

        dialogue.update(BotState::ProfilePages { msg_id }).await?;

        Ok(())
    }

    async fn handle_pages(bot: Bot, dialogue: BotDialogue, q: CallbackQuery, db: Db) -> HandlerResult {
        let msg_id = match dialogue.get_or_default().await? {
            BotState::ProfilePages { msg_id } => msg_id,
            _ => MessageId(0)
        };

        let cb = match q.clone().data {
            Some(data) => data,
            None => {
                dialogue.update(BotState::Profile { msg_id }).await?;
                return Ok(());
            }
        };

        let markup = InlineKeyboardMarkup::new(vec![
            vec![InlineKeyboardButton::callback("Назад", "back_btn")]
        ]);

        dialogue.update(BotState::Profile { msg_id }).await?;

        match cb.as_str() {
            "locate_btn" => {
                Self::handle_locate_btn(bot, dialogue.clone(), q.clone().chat_id().unwrap(), msg_id).await?;
            },
            "price_btn" => {
                Self::handle_price_btn(bot, dialogue.clone(), q.clone().chat_id().unwrap(), msg_id).await?;
            },
            "code_btn" => {
                Self::handle_code_btn(bot, q.from.id.0 as i64, q.clone().chat_id().unwrap(), msg_id, markup, db.clone()).await?;
            },
            "address_btn" => {
                Self::handle_address_btn(bot, q.from.id.0 as i64, q.clone().chat_id().unwrap(), msg_id, markup, db.clone()).await?;
            },
            "service_btn" => {
                Self::handle_service_btn(bot, q.chat_id().unwrap(), msg_id, markup).await?;
            },
            "tutorial_btn" => {
                Self::handle_tutorial_btn(bot, dialogue.clone(), q.chat_id().unwrap(), msg_id).await?;
            },
            _ => {
                Self::handle_invalid_query(bot, q.chat_id().unwrap(), msg_id, markup).await?;
            }
        };

        Ok(())
    }

    async fn handle_locate_btn(bot: Bot, dialogue: BotDialogue, chat_id: ChatId, msg_id: MessageId) -> HandlerResult {
        let message = "Введите трек-код товара";
        dialogue.update(BotState::ProductStatus { msg_id }).await?;

        bot.edit_message_text(chat_id, msg_id, message).await?.id;

        Ok(())
    }

    async fn get_product_status(bot: Bot, dialogue: BotDialogue, msg: Message) -> HandlerResult {
        let mut track_code = String::new();
        
        let markup = InlineKeyboardMarkup::new(
            vec![vec![InlineKeyboardButton::callback("Назад", "back_btn")]]
        );

        match msg.text() {
            Some(text) => {
                track_code = text.to_string();
            },
            None => {
                let msg_id = bot.send_message(msg.chat.id, indoc!(r#"
                Неверный формат.
                Введите трек-код еще раз.
                "#)).reply_markup(markup).await?.id;

                dialogue.update(BotState::ProductStatus { msg_id })
                    .await?;

                return Ok(());
            }
        };

        let message = if product_ready(track_code.as_str()).await {
            "Товар уже на складе, ждет сортировки"
        } else {
            "Товара еще нет на складе"
        };

        let msg_id = bot.send_message(msg.chat.id, message).reply_markup(markup).await?.id;

        dialogue.update(BotState::Profile { msg_id }).await?;

        Ok(())
    }

    async fn handle_price_btn(bot: Bot, dialogue: BotDialogue, chat_id: ChatId, msg_id: MessageId) -> HandlerResult {
        let message = "Введите ширину коробки с товаром (см)";

        bot.edit_message_text(chat_id, msg_id, message).await?;

        dialogue.update(BotState::PriceWidth).await?;

        Ok(())
    }

    async fn receive_width(bot: Bot, dialogue: BotDialogue, msg: Message) -> HandlerResult {
        let mut width = 0_f32;
        
        width = match msg.text() {
            Some(text) => {
                match text.to_string().parse::<f32>() {
                    Ok(num) => num,
                    Err(_) => {
                        bot.send_message(msg.chat.id, indoc!(r#"
                        Неверный формат.
                        Введите ширину еще раз.
                        "#)).await?;

                        dialogue.update(BotState::PriceWidth)
                        .await?;

                        return Ok(());
                    }
                }
            },
            None => {
                bot.send_message(msg.chat.id, indoc!(r#"
                Неверный формат.
                Введите ширину еще раз.
                "#)).await?;

                dialogue.update(BotState::PriceWidth)
                    .await?;

                return Ok(());
            }
        };

        bot.send_message(msg.chat.id, r#"
        Введите длину коробки с товаром (см)
        "#).await?;

        dialogue.update(BotState::PriceLength { width }).await?;

        Ok(())
    }

    async fn receive_length(bot: Bot, dialogue: BotDialogue, msg: Message) -> HandlerResult {
        let mut length = 0_f32;

        let width = match dialogue.get()
            .await?
            .expect("ERROR") {
                BotState::PriceLength { width } => width,
                _ => 0_f32
        };
        
        match msg.text() {
            Some(text) => {
                length = match text.to_string().parse::<f32>() {
                    Ok(num) => num,
                    Err(_) => {
                        bot.send_message(msg.chat.id, indoc!(r#"
                        Неверный формат.
                        Введите длину еще раз.
                        "#)).await?;

                        dialogue.update(BotState::PriceLength { width }).await?;

                        return Ok(());
                    }
                };
            },
            None => {
                bot.send_message(msg.chat.id, indoc!(r#"
                Неверный формат.
                Введите длину еще раз.
                "#)).await?;

                dialogue.update(BotState::PriceLength { width }).await?;

                return Ok(());
            }
        };

        bot.send_message(msg.chat.id, indoc!(r#"
        Введите высоту коробки с товаром (см)
        "#)).await?;

        dialogue.update(BotState::PriceHeight { width, length }).await?;

        Ok(())
    }

    async fn receive_height(bot: Bot, dialogue: BotDialogue, msg: Message) -> HandlerResult {
        let mut height = 0_f32;

        let (width, length) = match dialogue.get()
            .await?.unwrap() {
                BotState::PriceHeight { width, length }
                    => (width, length),
                _ => (0_f32, 0_f32)
        };

        match msg.text() {
            Some(text) => {
                height = match text.to_string().parse::<f32>() {
                    Ok(num) => num,
                    Err(_) => {
                        bot.send_message(msg.chat.id, indoc!(r#"
                        Неверный формат.
                        Введите высоту еще раз
                        "#)).await?;

                        dialogue.update(BotState::PriceHeight { width, length }).await?;

                        return Ok(());
                    }
                };
            },
            None => {
                bot.send_message(msg.chat.id, indoc!(r#"
                Неверный формат.
                Введите высоту еще раз
                "#)).await?;

                dialogue.update(BotState::PriceHeight { width, length }).await?;

                return Ok(());
            }
        };

        bot.send_message(msg.chat.id, "Введите вес коробки с товаром (кг)").await?;

        dialogue.update(BotState::PriceWeight { width, length, height }).await?;
        
        Ok(())
    }

    async fn receive_weight(bot: Bot, dialogue: BotDialogue, msg: Message) -> HandlerResult {
        let mut weight = 0_f32;

        let (width, length, height) = match dialogue.get()
            .await?.unwrap() {
                BotState::PriceWeight { width, length, height }
                    => (width, length, height),
                _ => (0_f32, 0_f32, 0_f32)
        };

        match msg.text() {
            Some(text) => {
                weight = match text.to_string().parse::<f32>() {
                    Ok(num) => num,
                    Err(_) => {
                        bot.send_message(msg.chat.id, indoc!(r#"
                        Неверный формат.
                        Введите вес еще раз
                        "#)).await?;

                        dialogue.update(BotState::PriceWeight { width, length, height }).await?;

                        return Ok(());
                    }
                };
            },
            None => {
                bot.send_message(msg.chat.id, indoc!(r#"
                Неверный формат.
                Введите вес еще раз
                "#)).await?;

                dialogue.update(BotState::PriceWeight { width, length, height }).await?;

                return Ok(());
            }
        };

        let volume = width * length * height * 0.000001;

        let density = weight / volume;

        let message = if density >= 100_f32 {
            format!("Плотность составляет: {} кг/м3.\nЦена товара высчитывается по весу", density)
        } else {
            format!("Плотность составляет: {} кг/м3.\nЦена товара высчитывается по плотности", density)
        };

        let markup = InlineKeyboardMarkup::new(
            vec![vec![InlineKeyboardButton::callback("Вернуться в личный кабинет", "back_btn")]]
        );
        
        let msg_id = bot.send_message(msg.chat.id, message).reply_markup(markup).await?.id;

        dialogue.update(BotState::Profile { msg_id }).await?;

        Ok(())
    }

    async fn handle_code_btn(bot: Bot, tg_id: i64, chat_id: ChatId, msg_id: MessageId, markup: InlineKeyboardMarkup, db: Db) -> HandlerResult {
        let client_code = db.get_user(tg_id).await.client_code;

        bot.edit_message_text(chat_id, msg_id, client_code).reply_markup(markup).await?;

        Ok(())
    }

    async fn handle_address_btn(bot: Bot, tg_id: i64, chat_id: ChatId, msg_id: MessageId, markup: InlineKeyboardMarkup, db: Db) -> HandlerResult {
        let client_code = db.get_user(tg_id).await.client_code;

        let message = format!(indoc!(r#"
        收件人：溴溴{}
        电话：18160860859
        地区：浙江省 金华市 义乌市 
        详细地址：江东街道东苑路45号一楼左侧 7号仓库(溴溴){}
        "#), client_code.as_str(), client_code.as_str());

        bot.edit_message_text(chat_id, msg_id, message).reply_markup(markup).await?;

        Ok(())
    }

    async fn handle_service_btn(bot: Bot, chat_id: ChatId, msg_id: MessageId, markup: InlineKeyboardMarkup) -> HandlerResult {
        let message = indoc!(r#"
        Контакты тех. поддержки:
        +996706518003
        "#);

        bot.edit_message_text(chat_id, msg_id, message).reply_markup(markup).await?;

        Ok(())
    }

    async fn handle_tutorial_btn(bot: Bot, dialogue: BotDialogue, chat_id: ChatId, msg_id: MessageId) -> HandlerResult {
        let markup = InlineKeyboardMarkup::new(
            vec![
            vec![
                InlineKeyboardButton::callback("1688", "1688_btn"),
                InlineKeyboardButton::callback("Pinduoduo", "pinduoduo_btn")
            ],
            vec![
                InlineKeyboardButton::callback("Poizon", "poizon_btn"),
                InlineKeyboardButton::callback("TaoBao", "taobao_btn")
            ]
            ]
        );

        let message = "Выберите маркетплейс, инструкцию к которой вы бы хотели получить";

        let msg_id = bot.edit_message_text(chat_id, msg_id, message)
            .reply_markup(markup)
            .await?.id;

        dialogue.update(BotState::Tutorial { msg_id }).await?;

        Ok(())
    }

    async fn handle_tutorials(bot: Bot, dialogue: BotDialogue, q: CallbackQuery) -> HandlerResult {
        let mut msg_id = match dialogue.get().await?.unwrap() {
            BotState::Tutorial { msg_id } => msg_id,
            _ => MessageId(0)
        };

        let message = match q.clone().data.unwrap().as_str() {
            "1688_btn" => format!(indoc!(r#"
                    Инструкция к 1688:
                    {}"#), env::var("HELP_1688")?),
            "pinduoduo_btn" => format!(indoc!(r#"
                    Инструкция к Pinduoduo:
                    {}"#), env::var("HELP_PINDUODUO")?),
            "poizon_btn" => format!(indoc!(r#"
                    Инструкция к Poizon:
                    {}"#), env::var("HELP_POIZON")?),
            "taobao_btn" => format!(indoc!(r#"
                    Инструкция к TaoBao:
                    {}"#), env::var("HELP_TAOBAO")?),
            _ => format!(indoc!("
                    Инструкция к TaoBao:
                    {}"), env::var("HELP_TAOBAO")?)
        };

        let markup = InlineKeyboardMarkup::new(
            vec![vec![InlineKeyboardButton::callback("Вернуться в личный кабинет", "back_btn")]]
        );

        let chat_id = q.clone().chat_id().unwrap();

        msg_id = bot.edit_message_text(chat_id, msg_id, message).reply_markup(markup).await?.id;

        dialogue.update(BotState::Profile { msg_id }).await?;

        Ok(())
    }

    async fn handle_invalid_query(bot: Bot, chat_id: ChatId, msg_id: MessageId, markup: InlineKeyboardMarkup) -> HandlerResult {
        bot.edit_message_text(chat_id, msg_id, "Произошла ошибка").reply_markup(markup).await?;
        
        Ok(())
    }

}
