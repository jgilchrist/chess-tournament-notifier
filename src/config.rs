use reqwest::Url;

pub struct Config {
    pub config_url: Url,
    pub notify_webhook: String,
    pub log_webhook: Option<String>,
}
