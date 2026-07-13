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

/// Minimum accepted refresh interval, in seconds.
const MIN_REFRESH_INTERVAL_SECS: u64 = 30;

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
    /// Handle used to persist config changes to disk.
    config_handler: Option<cosmic_config::Config>,
    /// Current balance information from the API.
    balance: Option<BalanceResponse>,
    /// Whether a network request is in-flight.
    loading: bool,
    /// Last error message, if any.
    error: Option<String>,
    /// When the balance was last successfully updated.
    last_updated: Option<chrono::DateTime<chrono::Local>>,
    /// Whether the settings form is shown instead of balance view.
    settings_open: bool,
    /// Draft value of the API key field while settings form is open.
    api_key_input: String,
    /// Draft value of the refresh interval field.
    interval_input: String,
    /// Whether the API key field is rendered in plain text.
    show_api_key: bool,
    /// Validation/persistence error inside the settings form.
    settings_error: Option<String>,
}

/// Messages emitted by the application and its widgets.
#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    BalanceFetched(Result<BalanceResponse, String>),
    UpdateConfig(Config),
    RefreshBalance,
    OpenSettings,
    CloseSettings,
    ApiKeyInputChanged(String),
    IntervalInputChanged(String),
    ToggleApiKeyVisibility,
    SaveSettings,
    PasteFromClipboard,
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
        let config_handler = match cosmic_config::Config::new(Self::APP_ID, Config::VERSION) {
            Ok(context) => Some(context),
            Err(why) => {
                tracing::warn!(%why, "failed to open config, using defaults");
                None
            }
        };

        let mut config = config_handler.as_ref().map_or_else(Config::default, |context| {
            match Config::get_entry(context) {
                Ok(config) => config,
                Err((_errors, config)) => {
                    tracing::warn!("config load had errors, using defaults");
                    config
                }
            }
        });

        // Fallback to DEEPSEEK_API_KEY environment variable.
        if config.api_key.is_empty()
            && let Ok(env_key) = std::env::var("DEEPSEEK_API_KEY")
        {
            tracing::info!("loaded DEEPSEEK_API_KEY from environment");
            config.api_key = env_key;
        }

        let api_key_input = config.api_key.clone();
        let interval_input = config.refresh_interval_secs.to_string();

        let app = AppModel {
            core,
            config,
            config_handler,
            api_key_input,
            interval_input,
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
        let text = widget::container(
            widget::text(format!(" {balance_str}"))
                .font(cosmic::iced::Font::with_name("Ubuntu Mono"))
                .size(13),
        )
        .height(cosmic::iced::Length::Fill)
        .align_y(cosmic::iced::alignment::Vertical::Center);

        let row = widget::row(vec![icon.into(), text.into()])
            .spacing(2)
            .align_y(cosmic::iced::Alignment::Center);

        let button = widget::button::custom(row)
            .class(cosmic::theme::Button::AppletIcon)
            .on_press(Message::TogglePopup)
            .padding([2, 6]);

        self.core.applet.autosize_window(button).into()
    }

    /// The applet's popup window — either settings form or balance view.
    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        if self.settings_open {
            return self.view_settings();
        }

        let mut content = widget::list_column();

        // --- Header: title + spacer + settings gear ---
        let header = widget::row(vec![
            widget::text::heading(fl!("balance-title")).into(),
            widget::space::horizontal().into(),
            {
                let gear_handle: widget::icon::Handle =
                    cosmic::widget::icon::from_name("emblem-system-symbolic").into();
                widget::button::custom(widget::icon::icon(gear_handle).size(16))
                    .on_press(Message::OpenSettings)
                    .class(cosmic::theme::Button::Icon)
                    .into()
            },
        ])
        .align_y(cosmic::iced::Alignment::Center);

        content = content.add(header);

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

        // --- Balance details ---
        if let Some(ref balance) = self.balance {
            let mono = |t: &str| -> Element<'_, Self::Message> {
                widget::text(t.to_owned()).font(cosmic::font::mono()).size(12).into()
            };

            for info in &balance.balance_infos {
                content = content.add(widget::settings::item(
                    format!("  {} ({})", fl!("total"), info.currency),
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

        // --- Footer: time (left) + spacer + refresh (right edge) ---
        let has_clock = self.last_updated.is_some();
        let has_refresh = !self.config.api_key.is_empty();

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

        if has_clock || has_refresh {
            footer_children.push(widget::space::horizontal().into());
        }

        if has_refresh {
            let refresh_handle: widget::icon::Handle =
                cosmic::widget::icon::from_name("view-refresh-symbolic").into();
            footer_children.push(
                widget::button::custom(widget::icon::icon(refresh_handle).size(14))
                    .on_press(Message::RefreshBalance)
                    .class(cosmic::theme::Button::Icon)
                    .into(),
            );
        }

        if has_clock || has_refresh {
            content = content.add(
                widget::flex_row(footer_children)
                    .align_items(cosmic::iced::Alignment::Center),
            );
        }

        self.core.applet.popup_container(content).into()
    }

    /// Register subscriptions for this application.
    fn subscription(&self) -> Subscription<Self::Message> {
        let api_key = self.config.api_key.clone();
        let interval_secs = self.config.refresh_interval_secs.max(MIN_REFRESH_INTERVAL_SECS);

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
                // Keep drafts in sync if settings not currently open.
                if !self.settings_open {
                    self.api_key_input = self.config.api_key.clone();
                    self.interval_input = self.config.refresh_interval_secs.to_string();
                }
            }
            Message::OpenSettings => {
                self.settings_open = true;
                self.show_api_key = false;
                self.settings_error = None;
                self.api_key_input = self.config.api_key.clone();
                self.interval_input = self.config.refresh_interval_secs.to_string();
            }
            Message::CloseSettings => {
                self.settings_open = false;
                self.settings_error = None;
                // Discard drafts, restore from last-saved config.
                self.api_key_input = self.config.api_key.clone();
                self.interval_input = self.config.refresh_interval_secs.to_string();
            }
            Message::ApiKeyInputChanged(value) => {
                self.api_key_input = value;
            }
            Message::IntervalInputChanged(value) => {
                // Keep only digits so the field is always parseable.
                self.interval_input = value.chars().filter(char::is_ascii_digit).take(6).collect();
            }
            Message::ToggleApiKeyVisibility => {
                self.show_api_key = !self.show_api_key;
            }
            Message::PasteFromClipboard => {
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        // Append pasted text to current value
                        self.api_key_input.push_str(&text);
                    }
                }
            }
            Message::SaveSettings => {
                let interval = match self.interval_input.parse::<u64>() {
                    Ok(v) if v >= MIN_REFRESH_INTERVAL_SECS => v,
                    Ok(_) => {
                        self.settings_error = Some(fl!(
                            "interval-too-small",
                            min = MIN_REFRESH_INTERVAL_SECS.to_string()
                        ));
                        return Task::none();
                    }
                    Err(_) => {
                        self.settings_error = Some(fl!("interval-invalid"));
                        return Task::none();
                    }
                };

                let api_key = self.api_key_input.trim().to_string();

                self.config.api_key = api_key.clone();
                self.config.refresh_interval_secs = interval;

                if let Some(handler) = &self.config_handler {
                    if let Err(why) = self.config.write_entry(handler) {
                        tracing::warn!(%why, "failed to persist config");
                        self.settings_error = Some(fl!("save-failed"));
                        return Task::none();
                    }
                } else {
                    tracing::warn!(
                        "no config handler, changes will not persist across restarts"
                    );
                }

                self.settings_open = false;
                self.settings_error = None;
                self.error = None;

                if api_key.is_empty() {
                    return Task::none();
                }

                self.loading = true;
                return cosmic::task::future(async move {
                    let result = api::fetch_balance(&api_key).await;
                    Message::BalanceFetched(result)
                });
            }
            Message::TogglePopup => {
                let Some(main_id) = self.core.main_window_id() else {
                    tracing::error!("no main window, cannot open popup");
                    return Task::none();
                };
                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        main_id,
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
                // Don't leave settings form open for next popup open.
                self.settings_open = false;
                self.settings_error = None;
            }
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
            Some(cosmic::applet::style())
        }
    }

    impl AppModel {
        /// Renders the settings form (API key + refresh interval).
        fn view_settings(&self) -> Element<'_, Message> {
            let mut content = widget::list_column();

            content = content.add(widget::text::heading(fl!("settings-title")));

            // --- API key field, with show/hide toggle + paste button ---
            let api_key_field = widget::text_input(fl!("api-key-placeholder"), &self.api_key_input)
                .on_input(Message::ApiKeyInputChanged)
                .on_paste(Message::ApiKeyInputChanged)
                .width(cosmic::iced::Length::Fill);
            let api_key_field = if self.show_api_key {
                api_key_field
            } else {
                api_key_field.password()
            };

            let eye_icon_name = if self.show_api_key {
                "view-conceal-symbolic"
            } else {
                "view-reveal-symbolic"
            };
            let eye_handle: widget::icon::Handle =
                cosmic::widget::icon::from_name(eye_icon_name).into();
            let eye_button = widget::button::custom(widget::icon::icon(eye_handle).size(14))
                .on_press(Message::ToggleApiKeyVisibility)
                .class(cosmic::theme::Button::Icon);

            // Paste-from-clipboard button (Wayland workaround)
            let paste_handle: widget::icon::Handle =
                cosmic::widget::icon::from_name("edit-paste-symbolic").into();
            let paste_button = widget::button::custom(widget::icon::icon(paste_handle).size(14))
                .on_press(Message::PasteFromClipboard)
                .class(cosmic::theme::Button::Icon);

            let api_key_row = widget::row(vec![
                api_key_field.into(),
                paste_button.into(),
                eye_button.into(),
            ])
            .spacing(4)
            .align_y(cosmic::iced::Alignment::Center);

            content = content.add(widget::settings::item(fl!("api-key-label"), api_key_row));

            // --- Refresh interval field ---
            let interval_field = widget::text_input("180", &self.interval_input)
                .on_input(Message::IntervalInputChanged)
                .on_submit(|_| Message::SaveSettings)
                .width(cosmic::iced::Length::Fixed(90.0));

            content = content.add(widget::settings::item(
                fl!("refresh-interval-label"),
                widget::row(vec![
                    interval_field.into(),
                    widget::text::body(fl!("seconds-suffix")).into(),
                ])
                .spacing(6)
                .align_y(cosmic::iced::Alignment::Center),
            ));

            // --- Validation / persistence error ---
            if let Some(err) = &self.settings_error {
                content = content.add(widget::text::body(err.clone()));
            }

            // --- Cancel / Save buttons ---
            let buttons = widget::row(vec![
                widget::space::horizontal().into(),
                widget::button::standard(fl!("cancel"))
                    .on_press(Message::CloseSettings)
                    .into(),
                widget::button::suggested(fl!("save"))
                    .on_press(Message::SaveSettings)
                    .into(),
            ])
            .spacing(8);

            content = content.add(buttons);

            self.core.applet.popup_container(content).into()
        }
    }
