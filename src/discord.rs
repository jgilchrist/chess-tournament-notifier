use anyhow::Result;
use serde_json::{json, Value};

pub fn send_message(webhook_url: &str, username: &str, message: &str) -> Result<()> {
    call_webhook(
        webhook_url,
        json!({
            "username": username,
            "allowed_mentions": { "parse": ["users"] },
            "content": message
        }),
    )
}

pub fn send_log_message(webhook_url: &str, prefix: &str, message: &str) -> Result<()> {
    send_message(webhook_url, prefix, &format!("[{}] {}", prefix, message))
}

fn call_webhook(webhook_url: &str, body: Value) -> Result<()> {
    let client = reqwest::blocking::Client::new();

    client
        .post(webhook_url)
        .json(&body)
        .send()?
        .error_for_status()?;

    Ok(())
}
