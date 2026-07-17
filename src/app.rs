// SPDX-License-Identifier: MPL-2.0 (Mozilla Public License 2.0)

use crate::api::{self, BalanceInfo, BalanceResponse};
use crate::config::Config;
use crate::fl;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::futures::SinkExt;
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{Alignment, Background, Color, Length, Limits, Subscription, window::Id};
use cosmic::prelude::*;
use cosmic::widget;

const MIN_REFRESH_INTERVAL_SECS: u64 = 30;
const STALE_THRESHOLD_MINS: i64 = 5;

fn primary_info(balance: &BalanceResponse) -> Option<&BalanceInfo> {
    balance
        .balance_infos
        .iter()
        .find(|b| b.currency == "USD")
        .or_else(|| balance.balance_infos.first())
}

fn parse_amount(s: &str) -> f64 {
    s.trim().parse::<f64>().unwrap_or(0.0)
}

fn currency_symbol(currency: &str) -> &str {
    match currency {
        "USD" => "$",
        "CNY" => "¥",
        "EUR" => "€",
        other => other,
    }
}

// ── styling helpers ──────────────────────────────────────────────────────────

fn apply_alpha(mut color: Color, opacity: f32) -> Color {
    color.a *= opacity;
    color
}

fn card_style(theme: &cosmic::Theme) -> widget::container::Style {
    let cosmic = theme.cosmic();
    widget::container::Style {
        text_color: None,
        background: Some(Background::Color(
            cosmic.background(false).component.base.into(),
        )),
        border: cosmic::iced::Border {
            radius: cosmic.corner_radii.radius_m.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        shadow: cosmic::iced::Shadow::default(),
        icon_color: None,
        snap: false,
    }
}

fn muted_card_style(theme: &cosmic::Theme) -> widget::container::Style {
    let cosmic = theme.cosmic();
    widget::container::Style {
        text_color: None,
        background: Some(Background::Color(apply_alpha(
            cosmic.background(false).component.base.into(),
            0.6,
        ))),
        border: cosmic::iced::Border {
            radius: cosmic.corner_radii.radius_m.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        shadow: cosmic::iced::Shadow::default(),
        icon_color: None,
        snap: false,
    }
}

fn card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    widget::container(content)
        .padding([12, 16])
        .style(card_style)
        .width(Length::Fill)
        .into()
}

fn info_block(
    title: String,
    message: String,
    action: Option<Element<'_, Message>>,
) -> Element<'_, Message> {
    let mut col = widget::list_column();
    col = col.add(widget::text::caption(title));
    col = col.add(widget::text::body(message));
    if let Some(btn) = action {
        col = col.add(btn);
    }
    widget::container(col)
        .padding([12, 16])
        .style(muted_card_style)
        .width(Length::Fill)
        .into()
}

fn badge_success(label: impl Into<String>) -> Element<'static, Message> {
    let label = label.into();
    badge_container(label, move |theme| {
        let cosmic = theme.cosmic();
        let color = cosmic.success.base.into();
        badge_style(apply_alpha(color, 0.14), color, color, theme)
    })
}

fn badge_warning(label: impl Into<String>) -> Element<'static, Message> {
    let label = label.into();
    badge_container(label, move |theme| {
        let cosmic = theme.cosmic();
        let color = cosmic.warning.base.into();
        badge_style(apply_alpha(color, 0.14), color, color, theme)
    })
}

fn badge_destructive(label: impl Into<String>) -> Element<'static, Message> {
    let label = label.into();
    badge_container(label, move |theme| {
        let cosmic = theme.cosmic();
        let color = cosmic.destructive.base.into();
        badge_style(apply_alpha(color, 0.14), color, color, theme)
    })
}

fn badge_neutral(label: impl Into<String>) -> Element<'static, Message> {
    let label = label.into();
    badge_container(label, move |theme| {
        let cosmic = theme.cosmic();
        let surface = &cosmic.background(false).component;
        badge_style(
            apply_alpha(surface.base.into(), 0.42),
            surface.on.into(),
            surface.divider.into(),
            theme,
        )
    })
}

fn badge_with_tooltip(
    badge: Element<'static, Message>,
    tooltip: impl Into<String>,
) -> Element<'static, Message> {
    widget::tooltip::tooltip(
        badge,
        widget::text(tooltip.into()).size(12),
        widget::tooltip::Position::Top,
    )
    .into()
}

fn badge_container(
    label: String,
    style: impl Fn(&cosmic::Theme) -> widget::container::Style + 'static,
) -> Element<'static, Message> {
    Element::from(
        widget::container(widget::text(label).size(12))
            .padding([3, 7])
            .style(style),
    )
}

fn badge_style(
    bg: Color,
    text_color: Color,
    border_color: Color,
    theme: &cosmic::Theme,
) -> widget::container::Style {
    let cosmic = theme.cosmic();
    widget::container::Style {
        text_color: Some(text_color),
        background: Some(Background::Color(bg)),
        border: cosmic::iced::Border {
            radius: cosmic.corner_radii.radius_s.into(),
            width: 1.0,
            color: border_color,
        },
        shadow: cosmic::iced::Shadow::default(),
        icon_color: None,
        snap: true,
    }
}

fn format_updated_label(last_updated: chrono::DateTime<chrono::Local>) -> String {
    let age = chrono::Local::now() - last_updated;
    if age.num_seconds() < 10 {
        fl!("updated-just-now")
    } else if age.num_minutes() < 1 {
        fl!("updated-seconds-ago", n = age.num_seconds())
    } else if age.num_hours() < 1 {
        fl!("updated-minutes-ago", n = age.num_minutes())
    } else {
        let date = last_updated.format("%H:%M").to_string();
        fl!("updated-at", date = date.as_str())
    }
}

#[derive(Default)]
pub struct AppModel {
    core: cosmic::Core,
    popup: Option<Id>,
    config: Config,
    config_handler: Option<cosmic_config::Config>,
    balance: Option<BalanceResponse>,
    loading: bool,
    error: Option<String>,
    last_updated: Option<chrono::DateTime<chrono::Local>>,
    settings_open: bool,
    api_key_input: String,
    interval_input: String,
    show_api_key: bool,
    settings_error: Option<String>,
}

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
    ToggleLanguage,
    PasteFromClipboard,
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "com.github.serhio.DeepSeekBalance";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

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
        let mut config = config_handler
            .as_ref()
            .map_or_else(Config::default, |context| {
                match Config::get_entry(context) {
                    Ok(config) => config,
                    Err((_errors, config)) => {
                        tracing::warn!("config load had errors, using defaults");
                        config
                    }
                }
            });
        if config.api_key.is_empty()
            && let Ok(env_key) = std::env::var("DEEPSEEK_API_KEY")
        {
            tracing::info!("loaded DEEPSEEK_API_KEY from environment");
            config.api_key = env_key;
        }
        let api_key_input = config.api_key.clone();
        let interval_input = config.refresh_interval_secs.to_string();
        // Apply saved language
        let lang_id: i18n_embed::unic_langid::LanguageIdentifier = config
            .language
            .parse()
            .unwrap_or_else(|_| "en".parse().unwrap());
        crate::i18n::init(&[lang_id]);
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

    fn view(&self) -> Element<'_, Self::Message> {
        let auth_err = self.error.as_deref() == Some("AUTH_ERROR");
        let offline = self
            .error
            .as_ref()
            .is_some_and(|e| e.starts_with("network error:"));
        let no_key = self.config.api_key.is_empty();
        let is_err = offline || auth_err || no_key;
        let balance_str: String = if no_key {
            fl!("badge-set-api-key")
        } else if offline {
            fl!("badge-no-network")
        } else if auth_err {
            fl!("badge-auth-error")
        } else if let Some(ref balance) = self.balance {
            match primary_info(balance) {
                Some(i) => format!("{}{}", currency_symbol(&i.currency), i.total_balance),
                None => "--".into(),
            }
        } else if self.loading {
            "···".into()
        } else {
            "--".into()
        };

        let icon_color =
            self.core
                .applet
                .theme()
                .map_or(cosmic::iced::Color::from_rgb(1.0, 1.0, 1.0), |t| {
                    let a = t.cosmic().accent_color();
                    cosmic::iced::Color::from_rgba(a.red, a.green, a.blue, a.alpha)
                });

        let svg_handle = cosmic::widget::svg::Handle::from_memory(include_bytes!(
            "../resources/deepseek-50.svg"
        ));
        let icon = cosmic::widget::Svg::new(svg_handle)
            .content_fit(cosmic::iced::ContentFit::Contain)
            .width(Length::Fixed(20.0))
            .height(Length::Fixed(20.0))
            .class(cosmic::theme::Svg::Custom(std::rc::Rc::new(
                move |_theme: &cosmic::Theme| cosmic::widget::svg::Style {
                    color: Some(icon_color),
                },
            )));
        let label_text = widget::text(format!(" {balance_str}"))
            .font(cosmic::iced::Font::with_name("Ubuntu Mono"))
            .size(13);
        let label = if is_err {
            let color = if offline {
                cosmic::iced::Color::from_rgb(0.95, 0.25, 0.25)
            } else {
                cosmic::iced::Color::from_rgb(0.90, 0.70, 0.10)
            };
            widget::container(label_text).style(move |_theme: &cosmic::Theme| {
                cosmic::widget::container::Style {
                    text_color: Some(color),
                    background: None,
                    border: cosmic::iced::Border {
                        radius: 0.0.into(),
                        width: 0.0,
                        color: cosmic::iced::Color::TRANSPARENT,
                    },
                    shadow: cosmic::iced::Shadow::default(),
                    icon_color: None,
                    snap: false,
                }
            })
        } else {
            widget::container(label_text)
        }
        .height(Length::Fill)
        .align_y(cosmic::iced::alignment::Vertical::Center);
        let row = widget::row(vec![icon.into(), label.into()])
            .spacing(2)
            .align_y(Alignment::Center);

        let suggested = self.core.applet.suggested_size(true);
        let (major, minor) = self.core.applet.suggested_padding(true);
        let (h_pad, v_pad) = if self.core.applet.is_horizontal() {
            (major, minor)
        } else {
            (minor, major)
        };
        let button = widget::button::custom(
            widget::container(row).center_y(Length::Fixed(f32::from(suggested.1 + 2 * v_pad))),
        )
        .on_press(Message::TogglePopup)
        .padding([0, h_pad])
        .class(cosmic::theme::Button::AppletIcon);
        self.core.applet.autosize_window(button).into()
    }

    #[allow(clippy::too_many_lines)]
    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        if self.settings_open {
            return self.view_settings();
        }

        let mut body = widget::list_column();

        // ── Header: title + gear ────────────────────────────────────────────
        let header = widget::row(vec![
            widget::text(fl!("balance-title"))
                .size(18)
                .width(Length::Fill)
                .into(),
            {
                let gear_handle: widget::icon::Handle =
                    cosmic::widget::icon::from_name("emblem-system-symbolic").into();
                widget::button::custom(widget::icon::icon(gear_handle).size(16))
                    .on_press(Message::OpenSettings)
                    .class(cosmic::theme::Button::Icon)
                    .into()
            },
        ])
        .spacing(8)
        .align_y(Alignment::Center);
        body = body.add(header);

        // ── No API key ────────────────────────────────────────────────────────
        if self.config.api_key.is_empty() {
            body = body.add(info_block(
                fl!("no-api-key-title"),
                fl!("no-api-key"),
                Some(
                    widget::row(vec![
                        badge_warning(fl!("badge-set-api-key")),
                        widget::space::horizontal().width(8).into(),
                        widget::button::standard(fl!("open-settings"))
                            .on_press(Message::OpenSettings)
                            .into(),
                    ])
                    .into(),
                ),
            ));
            return self
                .core
                .applet
                .popup_container(widget::container(body).padding([12, 8]).width(Length::Fill))
                .into();
        }

        // ── Loading (first fetch) ─────────────────────────────────────────────
        if self.loading && self.balance.is_none() {
            body = body.add(card(
                widget::list_column()
                    .add(
                        widget::row(vec![
                            widget::text::body(fl!("loading"))
                                .width(Length::Fill)
                                .into(),
                            badge_with_tooltip(
                                badge_neutral(fl!("badge-loading")),
                                fl!("badge-loading-tooltip"),
                            ),
                        ])
                        .align_y(Alignment::Center),
                    )
                    .add(widget::determinate_linear(0.0).girth(Length::Fixed(4.0))),
            ));
            return self
                .core
                .applet
                .popup_container(widget::container(body).padding([12, 8]).width(Length::Fill))
                .into();
        }

        // ── Balance ───────────────────────────────────────────────────────────
        let has_balance = self.balance.is_some();
        let show_card = has_balance || self.error.is_some();
        if show_card {
            // API unavailable banner
            if let Some(ref balance) = self.balance
                && !balance.is_available
            {
                body = body.add(info_block(
                    fl!("api-unavailable-title"),
                    fl!("api-unavailable"),
                    None,
                ));
            }

            let (symbol, amount) = if let Some(ref balance) = self.balance {
                if let Some(info) = primary_info(balance) {
                    (
                        currency_symbol(&info.currency).to_string(),
                        info.total_balance.clone(),
                    )
                } else {
                    ("$".into(), "\u{2014}".into())
                }
            } else {
                ("$".into(), "\u{2014}".into())
            };

            let status_badge: Element<'_, Message> = {
                let stale = self.last_updated.is_none_or(|t| {
                    (chrono::Local::now() - t).num_minutes() >= STALE_THRESHOLD_MINS
                });
                if self.loading {
                    badge_with_tooltip(
                        badge_neutral(fl!("badge-loading")),
                        fl!("badge-loading-tooltip"),
                    )
                } else if self.error.as_deref() == Some("AUTH_ERROR") {
                    widget::button::custom(badge_with_tooltip(
                        badge_warning(fl!("badge-auth-error")),
                        fl!("bad-auth"),
                    ))
                    .on_press(Message::OpenSettings)
                    .class(cosmic::theme::Button::Icon)
                    .padding(0)
                    .into()
                } else if self
                    .error
                    .as_ref()
                    .is_some_and(|e| e.starts_with("network error:"))
                {
                    widget::button::custom(badge_with_tooltip(
                        badge_destructive(fl!("badge-no-network")),
                        fl!("offline"),
                    ))
                    .on_press(Message::RefreshBalance)
                    .class(cosmic::theme::Button::Icon)
                    .padding(0)
                    .into()
                } else if stale {
                    badge_with_tooltip(
                        badge_warning(fl!("badge-offline")),
                        fl!("badge-offline-tooltip"),
                    )
                } else {
                    badge_with_tooltip(
                        badge_success(fl!("badge-online")),
                        fl!("badge-online-tooltip"),
                    )
                }
            };

            // Main balance card
            let mut balance_items = widget::list_column();
            balance_items = balance_items.add(
                widget::row(vec![
                    widget::text(format!("{symbol}{amount}"))
                        .size(36)
                        .width(Length::Fill)
                        .into(),
                    status_badge,
                ])
                .align_y(Alignment::Center),
            );
            body = body.add(card(balance_items));

            // Spent today card (only with real balance)
            if has_balance && let Some(spent_today) = self.spent_today() {
                body = body.add(card(
                    widget::row(vec![
                        widget::text(fl!("spent-today-label"))
                            .size(14)
                            .width(Length::Fill)
                            .into(),
                        widget::text(format!("{symbol}{spent_today:.2}"))
                            .size(14)
                            .into(),
                    ])
                    .align_y(Alignment::Center),
                ));
            }
        }

        // ── Footer ────────────────────────────────────────────────────────────
        {
            let mut footer_items: Vec<Element<'_, Message>> = Vec::new();

            if let Some(updated) = self.last_updated {
                footer_items.push(widget::text::caption(format_updated_label(updated)).into());
            }

            footer_items.push(widget::space::horizontal().into());

            let interval_secs = self
                .config
                .refresh_interval_secs
                .max(MIN_REFRESH_INTERVAL_SECS);
            footer_items.push(
                widget::text::caption(format!(
                    "{} {}s",
                    fl!("refresh-interval-label-short"),
                    interval_secs
                ))
                .into(),
            );

            let refresh_handle: widget::icon::Handle =
                cosmic::widget::icon::from_name("view-refresh-symbolic").into();
            footer_items.push(
                widget::button::custom(widget::icon::icon(refresh_handle).size(14))
                    .on_press(Message::RefreshBalance)
                    .class(cosmic::theme::Button::Icon)
                    .into(),
            );

            body = body.add(
                widget::container(widget::row(footer_items).align_y(Alignment::Center))
                    .padding([4, 4, 4, 4])
                    .width(Length::Fill),
            );
        }

        self.core
            .applet
            .popup_container(widget::container(body).padding([12, 8]).width(Length::Fill))
            .into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let api_key = self.config.api_key.clone();
        let interval_secs = self
            .config
            .refresh_interval_secs
            .max(MIN_REFRESH_INTERVAL_SECS);
        Subscription::batch(vec![
            Subscription::run_with(
                (api_key, interval_secs),
                |(api_key, interval_secs): &(String, u64)| {
                    let api_key = api_key.clone();
                    let interval_secs = *interval_secs;
                    cosmic::iced::stream::channel(
                        4,
                        move |mut channel: cosmic::iced::futures::channel::mpsc::Sender<
                            Message,
                        >| async move {
                            if !api_key.is_empty() {
                                _ = channel
                                    .send(Message::BalanceFetched(
                                        api::fetch_balance(&api_key).await,
                                    ))
                                    .await;
                            }
                            loop {
                                tokio::time::sleep(std::time::Duration::from_secs(interval_secs))
                                    .await;
                                if !api_key.is_empty()
                                    && channel
                                        .send(Message::BalanceFetched(
                                            api::fetch_balance(&api_key).await,
                                        ))
                                        .await
                                        .is_err()
                                {
                                    break;
                                }
                            }
                        },
                    )
                },
            ),
            self.core()
                .watch_config::<Config>(Self::APP_ID)
                .map(|update| {
                    tracing::info!("config updated via dbus");
                    Message::UpdateConfig(update.config)
                }),
        ])
    }

    #[allow(clippy::too_many_lines)]
    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::BalanceFetched(result) => {
                self.loading = false;
                match result {
                    Ok(balance) => {
                        self.update_spend_baseline(&balance);
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
                    Message::BalanceFetched(api::fetch_balance(&api_key).await)
                });
            }
            Message::UpdateConfig(mut config) => {
                if config.api_key.is_empty()
                    && let Ok(env_key) = std::env::var("DEEPSEEK_API_KEY")
                {
                    config.api_key = env_key;
                }
                self.config = config;
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
                self.api_key_input = self.config.api_key.clone();
                self.interval_input = self.config.refresh_interval_secs.to_string();
            }
            Message::ApiKeyInputChanged(value) => {
                self.api_key_input = value;
            }
            Message::IntervalInputChanged(value) => {
                self.interval_input = value.chars().filter(char::is_ascii_digit).take(6).collect();
            }
            Message::ToggleApiKeyVisibility => {
                self.show_api_key = !self.show_api_key;
            }
            Message::PasteFromClipboard => {
                if let Ok(mut clipboard) = arboard::Clipboard::new()
                    && let Ok(text) = clipboard.get_text()
                {
                    self.api_key_input.push_str(&text);
                }
            }
            Message::ToggleLanguage => {
                let new_lang = if self.config.language == "ru" {
                    "en"
                } else {
                    "ru"
                };
                self.config.language = new_lang.into();
                let lang_id: i18n_embed::unic_langid::LanguageIdentifier =
                    self.config.language.parse().unwrap();
                crate::i18n::init(&[lang_id]);
                let _ = self.persist_config();
            }
            Message::SaveSettings => {
                let interval = self
                    .interval_input
                    .parse::<u64>()
                    .unwrap_or(MIN_REFRESH_INTERVAL_SECS);
                let interval = interval.max(MIN_REFRESH_INTERVAL_SECS);
                let api_key = self.api_key_input.trim().to_string();
                self.config.api_key.clone_from(&api_key);
                self.config.refresh_interval_secs = interval;
                if let Err(()) = self.persist_config() {
                    self.settings_error = Some(fl!("save-failed"));
                    return Task::none();
                }
                self.settings_open = false;
                self.settings_error = None;
                self.error = None;
                if api_key.is_empty() {
                    return Task::none();
                }
                self.loading = true;
                return cosmic::task::future(async move {
                    Message::BalanceFetched(api::fetch_balance(&api_key).await)
                });
            }
            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    let Some(main_id) = self.core.main_window_id() else {
                        tracing::error!("no main window id");
                        return Task::none();
                    };
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings = self
                        .core
                        .applet
                        .get_popup_settings(main_id, new_id, None, None, None);
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(420.0)
                        .min_width(300.0)
                        .min_height(180.0)
                        .max_height(800.0);
                    get_popup(popup_settings)
                };
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
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
    fn persist_config(&self) -> Result<(), ()> {
        let Some(handler) = &self.config_handler else {
            tracing::warn!("no config handler, changes won't persist");
            return Ok(());
        };
        if let Err(why) = self.config.write_entry(handler) {
            tracing::warn!(%why, "failed to persist config");
            return Err(());
        }
        Ok(())
    }

    fn update_spend_baseline(&mut self, balance: &BalanceResponse) {
        let Some(current) = primary_info(balance).map(|i| parse_amount(&i.total_balance)) else {
            return;
        };
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let baseline = parse_amount(&self.config.spend_day_start_balance);
        let needs_reset = self.config.spend_day != today;
        let topped_up = current > baseline;
        if needs_reset || topped_up {
            self.config.spend_day = today;
            self.config.spend_day_start_balance = format!("{current:.6}");
            let _ = self.persist_config();
        }
    }

    fn spent_today(&self) -> Option<f64> {
        let info = primary_info(self.balance.as_ref()?)?;
        let current = parse_amount(&info.total_balance);
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        if self.config.spend_day != today {
            return Some(0.0);
        }
        let baseline = parse_amount(&self.config.spend_day_start_balance);
        Some((baseline - current).max(0.0))
    }

    #[allow(clippy::too_many_lines)]
    fn view_settings(&self) -> Element<'_, Message> {
        let mut body = widget::list_column();

        // Header
        let back_handle: widget::icon::Handle =
            cosmic::widget::icon::from_name("go-previous-symbolic").into();
        body = body.add(
            widget::row(vec![
                widget::button::custom(widget::icon::icon(back_handle).size(16))
                    .on_press(Message::CloseSettings)
                    .class(cosmic::theme::Button::Icon)
                    .into(),
                widget::text(fl!("settings-title"))
                    .size(18)
                    .width(Length::Fill)
                    .into(),
            ])
            .spacing(8)
            .align_y(Alignment::Center),
        );

        // ── Settings card ──────────────────────────────────────────────────────
        let mut settings_list = widget::list_column();

        // API Key row
        let key_handle: widget::icon::Handle =
            cosmic::widget::icon::from_name("dialog-password-symbolic").into();
        let key_icon = widget::tooltip::tooltip(
            widget::icon::icon(key_handle).size(16),
            widget::text(fl!("api-key-description")).size(12),
            widget::tooltip::Position::Top,
        );
        let eye_label = if self.show_api_key { "***" } else { "abc" };
        let eye_button = widget::button::custom(widget::text(eye_label).size(11))
            .on_press(Message::ToggleApiKeyVisibility)
            .class(cosmic::theme::Button::Icon);
        let paste_handle: widget::icon::Handle =
            cosmic::widget::icon::from_name("edit-paste-symbolic").into();
        let paste_button = widget::button::custom(widget::icon::icon(paste_handle).size(14))
            .on_press(Message::PasteFromClipboard)
            .class(cosmic::theme::Button::Icon);
        let mut api_key_field = widget::text_input(fl!("api-key-placeholder"), &self.api_key_input)
            .on_input(Message::ApiKeyInputChanged)
            .on_paste(Message::ApiKeyInputChanged)
            .on_submit(|_| Message::SaveSettings)
            .width(Length::Fill);
        if !self.show_api_key {
            api_key_field = api_key_field.password();
        }
        settings_list = settings_list.add(
            widget::row(vec![
                key_icon.into(),
                api_key_field.into(),
                paste_button.into(),
                eye_button.into(),
            ])
            .spacing(6)
            .align_y(Alignment::Center),
        );

        // Interval row
        let timer_handle: widget::icon::Handle =
            cosmic::widget::icon::from_name("view-refresh-symbolic").into();
        let interval_field = widget::text_input("180", &self.interval_input)
            .on_input(Message::IntervalInputChanged)
            .on_submit(|_| Message::SaveSettings)
            .width(Length::Fixed(70.0));
        let interval_ok = self
            .interval_input
            .parse::<u64>()
            .map_or(true, |v| v >= MIN_REFRESH_INTERVAL_SECS);
        settings_list = settings_list.add(
            widget::row(vec![
                widget::icon::icon(timer_handle).size(16).into(),
                interval_field.into(),
                widget::text(fl!("seconds-suffix")).size(14).into(),
            ])
            .spacing(6)
            .align_y(Alignment::Center),
        );
        if !interval_ok {
            settings_list = settings_list.add(widget::text::caption(fl!(
                "interval-too-small",
                min = MIN_REFRESH_INTERVAL_SECS.to_string()
            )));
        }

        // Language row
        let lang_handle: widget::icon::Handle =
            cosmic::widget::icon::from_name("preferences-desktop-locale-symbolic").into();
        let current_lang = if self.config.language == "ru" {
            "RU"
        } else {
            "EN"
        };
        settings_list = settings_list.add(
            widget::row(vec![
                widget::icon::icon(lang_handle).size(16).into(),
                widget::button::standard(current_lang)
                    .on_press(Message::ToggleLanguage)
                    .padding([2, 8])
                    .into(),
                widget::space::horizontal().width(Length::Fill).into(),
            ])
            .spacing(6)
            .align_y(Alignment::Center),
        );

        body = body.add(card(settings_list));

        // ── Error ─────────────────────────────────────────────────────────────
        if let Some(err) = &self.settings_error {
            body = body.add(info_block(fl!("error-prefix"), err.clone(), None));
        }

        // ── Buttons ───────────────────────────────────────────────────────────
        body = body.add(
            widget::row(vec![
                widget::space::horizontal().width(Length::Fill).into(),
                widget::button::standard(fl!("cancel"))
                    .on_press(Message::CloseSettings)
                    .into(),
                widget::button::suggested(fl!("save"))
                    .on_press(Message::SaveSettings)
                    .into(),
            ])
            .spacing(8),
        );

        self.core
            .applet
            .popup_container(widget::container(body).padding([12, 8]).width(Length::Fill))
            .into()
    }
}
