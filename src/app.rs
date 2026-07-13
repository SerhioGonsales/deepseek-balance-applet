// SPDX-License-Identifier: MPL-2.0 (Mozilla Public License 2.0)

use crate::api::{self, BalanceResponse};
use crate::config::Config;
use crate::fl;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::futures::SinkExt;
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{window::Id, Limits, Subscription};
use cosmic::prelude::*;
use cosmic::widget;

/// The application model stores app-specific state used to describe its interface and
/// drive its logic.
#[derive(Default)]
pub struct AppModel {
    /// Application state which is managed by the COSMIC runtime.
    core: cosmic::Core,
    /// The popup id.
    popup: Option<Id>,
    /// Configuration data that persists between application runs.
    config: Config,
    /// Current balance information from the API.
    balance: Option<BalanceResponse>,
    /// Whether a network request is in-flight.
    loading: bool,
    /// Last error message, if any.
    error: Option<String>,
    /// When the balance was last successfully updated.
    last_updated: Option<chrono::DateTime<chrono::Local>>,
}

/// Messages emitted by the application and its widgets.
#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    BalanceFetched(Result<BalanceResponse, String>),
    UpdateConfig(Config),
    RefreshBalance,
}

/// Create a COSMIC application from the app model.
impl cosmic::Application for AppModel {
    /// The async executor that will be used to run your application's commands.
    type Executor = cosmic::executor::Default;

    /// Data that your application receives to its init method.
    type Flags = ();

    /// Messages which the application and its widgets will emit.
    type Message = Message;

    /// Unique identifier in RDNN (reverse domain name notation) format.
    const APP_ID: &'static str = "com.github.serhio.DeepSeekBalance";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    /// Initializes the application with any given flags and startup commands.
    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let mut config = cosmic_config::Config::new(Self::APP_ID, Config::VERSION).map_or_else(
            |why| {
                tracing::warn!(%why, "failed to open config, using defaults");
                Config::default()
            },
            |context| match Config::get_entry(&context) {
                Ok(config) => config,
                Err((_errors, config)) => {
                    tracing::warn!("config load had errors, using defaults");
                    config
                }
            },
        );

        // Fallback to DEEPSEEK_API_KEY environment variable.
        if config.api_key.is_empty()
            && let Ok(env_key) = std::env::var("DEEPSEEK_API_KEY")
        {
            tracing::info!("loaded DEEPSEEK_API_KEY from environment");
            config.api_key = env_key;
        }

        let app = AppModel {
            core,
            config,
            ..Default::default()
        };

        (app, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    /// The applet's button in the panel — icon + balance, auto-sized surface.
    fn view(&self) -> Element<'_, Self::Message> {
        let balance_str: String = if self.config.api_key.is_empty() {
            "?".into()
        } else if let Some(ref balance) = self.balance {
            let info = balance
                .balance_infos
                .iter()
                .find(|b| b.currency == "USD")
                .or_else(|| balance.balance_infos.first());
            match info {
                Some(i) => {
                    let s = match i.currency.as_str() {
                        "USD" => "$",
                        "CNY" => "¥",
                        "EUR" => "€",
                        _ => "",
                    };
                    format!("{s}{}", i.total_balance)
                }
                None => "--".into(),
            }
        } else if self.loading {
            "···".into()
        } else {
            "--".into()
        };

        let icon_handle = widget::icon::from_raster_bytes(
            include_bytes!("../resources/icons8-deepseek-48.png"),
        );
        let icon = widget::icon::icon(icon_handle).size(20);
        let text = widget::text(format!(" {balance_str}"))
            .size(12);

        let row = widget::row(vec![icon.into(), text.into()])
            .spacing(4)
            .align_y(cosmic::iced::Alignment::Center)
            .height(cosmic::iced::Length::Fill);

        let button = widget::button::custom(row)
            .class(cosmic::theme::Button::AppletIcon)
            .on_press(Message::TogglePopup)
            .padding([2, 6]);

        self.core.applet.autosize_window(button).into()
    }

    /// The applet's popup window — minimal two-section layout.
    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let mut content = widget::list_column();

        // --- Title with current USD balance ---
        let title = if let Some(ref balance) = self.balance {
            let usd = balance
                .balance_infos
                .iter()
                .find(|b| b.currency == "USD")
                .or_else(|| balance.balance_infos.first());
            match usd {
                Some(info) => format!("{} ${}", fl!("balance-title"), info.total_balance),
                None => fl!("balance-title"),
            }
        } else {
            fl!("balance-title")
        };
        content = content.add(widget::text::heading(title));

        // --- Config missing / Error / Loading / No data ---
        if self.config.api_key.is_empty() {
            content = content.add(widget::text::body(fl!("no-api-key")));
        } else if let Some(error) = &self.error {
            content = content.add(widget::text::body(format!(
                "{}: {error}",
                fl!("error-prefix")
            )));
        } else if self.loading && self.balance.is_none() {
            content = content.add(widget::text::body(fl!("loading")));
        } else if self.balance.is_none() {
            content = content.add(widget::text::body(fl!("no-data")));
        }

        // --- API availability warning ---
        if let Some(ref balance) = self.balance
            && !balance.is_available
        {
            content = content.add(widget::text::body("⚠ API reports unavailable"));
        }

        // --- Balance details (separate rows per field, per currency) ---
        if let Some(ref balance) = self.balance {
            for info in &balance.balance_infos {
                let currency_label = format!("{} ({})", info.currency, fl!("balance"));
                content = content.add(widget::text::body(currency_label));

                let mono = |t: &str| -> Element<'_, Self::Message> {
                    widget::text(t.to_owned()).font(cosmic::font::mono()).size(12).into()
                };

                content = content.add(widget::settings::item(
                    format!("  {}", fl!("total")),
                    mono(&info.total_balance),
                ));
                content = content.add(widget::settings::item(
                    format!("  {}", fl!("topped-up")),
                    mono(&info.topped_up_balance),
                ));
                content = content.add(widget::settings::item(
                    format!("  {}", fl!("granted")),
                    mono(&info.granted_balance),
                ));
            }
        }

        // --- Footer: last updated + refresh button ---
        let mut footer_children: Vec<Element<'_, Self::Message>> = Vec::new();

        if let Some(updated) = self.last_updated {
            footer_children.push(
                widget::text::caption(format!(
                    "{} {}",
                    fl!("last-updated"),
                    updated.format("%H:%M")
                ))
                .into(),
            );
        }

        if !self.config.api_key.is_empty() {
            footer_children.push(
                widget::button::standard(if self.loading {
                    fl!("loading")
                } else {
                    fl!("refresh")
                })
                .on_press(Message::RefreshBalance)
                .into(),
            );
        }

        if !footer_children.is_empty() {
            content = content.add(widget::flex_row(footer_children).align_items(cosmic::iced::Alignment::Center));
        }

        self.core.applet.popup_container(content).into()
    }

    /// Register subscriptions for this application.
    fn subscription(&self) -> Subscription<Self::Message> {
        let api_key = self.config.api_key.clone();
        let interval_secs = self.config.refresh_interval_secs.max(30);

        Subscription::batch(vec![
            // Periodic balance polling.
            Subscription::run_with(
                (api_key, interval_secs),
                |(api_key, interval_secs): &(String, u64)| {
                    let api_key = api_key.clone();
                    let interval_secs = *interval_secs;
                    cosmic::iced::stream::channel(
                        4,
                        move |mut channel: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                            // Fetch immediately on start.
                            if !api_key.is_empty() {
                                let result = api::fetch_balance(&api_key).await;
                                _ = channel.send(Message::BalanceFetched(result)).await;
                            }

                            // Then poll periodically.
                            loop {
                                tokio::time::sleep(std::time::Duration::from_secs(
                                    interval_secs,
                                ))
                                .await;
                                if !api_key.is_empty() {
                                    let result = api::fetch_balance(&api_key).await;
                                    if channel
                                        .send(Message::BalanceFetched(result))
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                            }
                        },
                    )
                },
            ),
            // Watch for application configuration changes.
            self.core()
                .watch_config::<Config>(Self::APP_ID)
                .map(|update| {
                    tracing::info!("config updated via dbus");
                    Message::UpdateConfig(update.config)
                }),
        ])
    }

    /// Handles messages emitted by the application and its widgets.
    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::BalanceFetched(result) => {
                self.loading = false;
                match result {
                    Ok(balance) => {
                        self.balance = Some(balance);
                        self.error = None;
                        self.last_updated = Some(chrono::Local::now());
                    }
                    Err(e) => {
                        tracing::warn!(%e, "balance fetch failed");
                        self.error = Some(e);
                    }
                }
            }
            Message::RefreshBalance => {
                if self.config.api_key.is_empty() {
                    self.error = Some(fl!("no-api-key"));
                    return Task::none();
                }
                self.loading = true;
                let api_key = self.config.api_key.clone();
                return cosmic::task::future(async move {
                    let result = api::fetch_balance(&api_key).await;
                    Message::BalanceFetched(result)
                });
            }
            Message::UpdateConfig(mut config) => {
                // Re-apply env var fallback if config has no key.
                if config.api_key.is_empty()
                    && let Ok(env_key) = std::env::var("DEEPSEEK_API_KEY")
                {
                    config.api_key = env_key;
                }
                self.config = config;
            }
            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(420.0)
                        .min_width(300.0)
                        .min_height(180.0)
                        .max_height(800.0);
                    get_popup(popup_settings)
                }
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}
