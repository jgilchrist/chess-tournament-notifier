use anyhow::Result;
use serde_json::{json, Value};

pub fn send_message(webhook_url: &str, message: &str) -> Result<()> {
    call_webhook(
        webhook_url,
        json!({
            "username": "tcec-notifier",
            "allowed_mentions": { "parse": ["users"] },
            "content": message
        }),
    )
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
