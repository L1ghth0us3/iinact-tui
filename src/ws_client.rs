use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::sleep;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::model::{AppEvent, AppState};
use crate::parse::parse_combat_data;

pub async fn run(ws_url: String, tx: UnboundedSender<AppEvent>, _state: std::sync::Arc<tokio::sync::RwLock<AppState>>) {
    // Simple reconnect loop
    loop {
        match connect_async(&ws_url).await {
            Ok((ws_stream, _resp)) => {
                let (mut write, mut read) = ws_stream.split();
                let _ = tx.send(AppEvent::Connected);

                // Perform handshake: getLanguage, then subscribe
                let _ = write
                    .send(Message::Text("{\"call\":\"getLanguage\"}".to_string()))
                    .await;
                let _ = write
                    .send(Message::Text(
                        "{\"call\":\"subscribe\",\"events\":[\"CombatData\",\"LogLine\"]}".to_string(),
                    ))
                    .await;

                // Reader loop
                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(txt)) => {
                            if let Ok(val) = serde_json::from_str::<Value>(&txt) {
                                if let Some((enc, rows)) = parse_combat_data(&val) {
                                    let _ = tx.send(AppEvent::CombatData { encounter: enc, rows });
                                }
                            }
                        }
                        Ok(Message::Binary(_)) => {}
                        Ok(Message::Ping(_)) => {}
                        Ok(Message::Pong(_)) => {}
                        Ok(Message::Close(_)) => break,
                        Err(_e) => break,
                    }
                }
                let _ = tx.send(AppEvent::Disconnected);
            }
            Err(_e) => {
                let _ = tx.send(AppEvent::Disconnected);
            }
        }

        // Backoff before reconnect
        sleep(Duration::from_secs(2)).await;
    }
}

