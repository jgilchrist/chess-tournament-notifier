use super::ccc_pgn::{self, Game};
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};

const CCC_WS_URL: &str = "wss://ccc-api.gcp-prod.chess.com/ws";
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

fn collect_games(payload: &Value, games: &mut Vec<Game>) -> Result<()> {
    let events = payload
        .as_array()
        .map(|items| items.as_slice())
        .unwrap_or(std::slice::from_ref(payload));
    for event in events {
        if event.get("type").and_then(Value::as_str) != Some("gameUpdate") {
            continue;
        }
        if let Some(pgn) = event.pointer("/gameDetails/pgn").and_then(Value::as_str) {
            games.push(ccc_pgn::parse(pgn)?);
        }
    }
    Ok(())
}

pub async fn get_current_games() -> Result<Vec<Game>> {
    let (mut socket, _) = connect_async(CCC_WS_URL).await?;
    socket
        .send(Message::Text(r#"{"type":"requestEvent"}"#.into()))
        .await?;

    let deadline = tokio::time::Instant::now() + RESPONSE_TIMEOUT;
    let mut games = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, socket.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                let payload: Value = serde_json::from_str(&text)?;
                collect_games(&payload, &mut games)?;
            }
            Ok(Some(Ok(Message::Binary(bytes)))) => {
                let payload: Value = serde_json::from_slice(&bytes)?;
                collect_games(&payload, &mut games)?;
            }
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Err(_) => break,
            Ok(Some(Ok(_))) => {}
            Ok(Some(Err(error))) => return Err(error.into()),
        }
    }
    socket.close(None).await.ok();
    games.sort_by_key(Game::hash);
    games.dedup_by_key(|game| game.hash());
    Ok(games)
}
