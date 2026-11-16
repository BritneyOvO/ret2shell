use std::{path::Path, time::Duration};

use async_nats::jetstream::AckKind;
use futures::StreamExt;
use r2s_database::game;
use r2s_queue::TracedMessage;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::traits::GlobalState;

const TOPIC: &str = "attachment_oss";

#[derive(Serialize, Deserialize)]
struct SyncRequest {
  game_id: i64,
  bucket: String,
  old: String,
  new: String,
}

pub async fn enqueue(state: &GlobalState, game: &game::Model, old: &str, new: &str, trace: &str) {
  if state.attachment_oss.is_none() || old == new {
    return;
  }
  let Some(bucket) = &game.bucket else { return };
  let request = SyncRequest {
    game_id: game.id,
    bucket: bucket.clone(),
    old: old.to_owned(),
    new: new.to_owned(),
  };
  // Ensure the stream exists even when the worker has not started yet.
  if let Err(err) = state.queue.subscribe(TOPIC).await {
    warn!(game_id=game.id, error=?err, "failed to initialize OSS sync queue");
  } else if let Err(err) = state.queue.publish_ack(TOPIC, request, trace).await {
    warn!(game_id=game.id, error=?err, "failed to enqueue OSS sync");
  }
}

pub async fn queue_worker(state: GlobalState) {
  loop {
    let mut messages = match state.queue.subscribe(TOPIC).await {
      Ok(messages) => messages,
      Err(err) => {
        warn!(error=?err, "failed to subscribe to OSS sync queue");
        tokio::time::sleep(Duration::from_secs(10)).await;
        continue;
      }
    };
    while let Some(result) = messages.next().await {
      let message = match result {
        Ok(message) => message,
        Err(err) => {
          warn!(error=?err, "OSS sync queue error");
          break;
        }
      };
      let request = match serde_json::from_slice::<TracedMessage<SyncRequest>>(&message.payload) {
        Ok(request) => request.payload,
        Err(err) => {
          warn!(error=?err, "invalid OSS sync request");
          message.double_ack().await.ok();
          continue;
        }
      };
      let sync = process_request(&state, &request);
      tokio::pin!(sync);
      let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
      let result = loop {
        tokio::select! {
          result = &mut sync => break result,
          _ = heartbeat.tick() => {
            if let Err(err) = message.ack_with(AckKind::Progress).await {
              warn!(error=?err, "failed to extend OSS sync acknowledgement");
            }
          }
        }
      };
      match result {
        Ok(()) => {
          message.double_ack().await.ok();
        }
        Err(err) => {
          warn!(game_id=request.game_id, error=?err, "OSS sync failed; retrying");
          message
            .ack_with(AckKind::Nak(Some(Duration::from_secs(30))))
            .await
            .ok();
        }
      }
    }
    tokio::time::sleep(Duration::from_secs(10)).await;
  }
}

async fn process_request(state: &GlobalState, request: &SyncRequest) -> anyhow::Result<()> {
  let Some(game) = game::get_by_bucket(&state.db.conn, &request.bucket).await? else {
    return Ok(());
  };
  if game.id != request.game_id {
    return Ok(());
  }
  let bucket = state.bucket.at(&request.bucket).await?;
  let Some(config) = &state.config.bucket else {
    return Ok(());
  };
  let Some(oss) = &state.attachment_oss else {
    return Ok(());
  };
  // Resolve the repository via Bucket first, which checks its name and path.
  drop(bucket);
  oss
    .sync_changes(
      request.game_id,
      &Path::new(&config.path).join(&request.bucket),
      &request.old,
      &request.new,
    )
    .await
}

pub async fn sync_worker(state: GlobalState) {
  let Some(oss) = &state.attachment_oss else {
    return;
  };
  let Some(bucket) = &state.config.bucket else {
    return;
  };
  let mut ticker = tokio::time::interval(Duration::from_secs(24 * 60 * 60));
  loop {
    ticker.tick().await;
    let games = match game::Entity::find()
      .filter(game::Column::Bucket.is_not_null())
      .all(&state.db.conn)
      .await
    {
      Ok(games) => games,
      Err(err) => {
        warn!(error=?err, "failed to load games for OSS sync");
        continue;
      }
    };
    for game in games {
      let Some(name) = game.bucket.as_deref() else {
        continue;
      };
      if let Err(err) = oss
        .sync_repository(game.id, &Path::new(&bucket.path).join(name))
        .await
      {
        warn!(game_id=game.id, error=?err, "failed to sync attachments to OSS");
      }
    }
  }
}
