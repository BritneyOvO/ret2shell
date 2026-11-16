use std::time::Duration;

use async_nats::jetstream::AckKind;
use futures::StreamExt;
use r2s_database::media;
use r2s_queue::TracedMessage;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use tracing::warn;

use crate::traits::GlobalState;

const TOPIC: &str = "media_oss";

pub async fn enqueue(state: &GlobalState, hash: &str) {
  if !state
    .attachment_oss
    .as_ref()
    .is_some_and(|oss| oss.media_enabled())
  {
    return;
  }
  if let Err(err) = state.queue.subscribe(TOPIC).await {
    warn!(error=?err, "failed to initialize media OSS queue");
  } else if let Err(err) = state.queue.publish_ack(TOPIC, hash, "media-upload").await {
    warn!(error=?err, "failed to enqueue media OSS sync");
  }
}

async fn sync_one(state: &GlobalState, hash: &str) -> anyhow::Result<()> {
  if media::get_by_hash(&state.db.conn, hash).await?.is_none() {
    return Ok(());
  }
  if let Some(oss) = &state.attachment_oss {
    oss.sync_media(&state.media, hash).await?;
  }
  Ok(())
}

pub async fn queue_worker(state: GlobalState) {
  loop {
    let mut messages = match state.queue.subscribe(TOPIC).await {
      Ok(messages) => messages,
      Err(err) => {
        warn!(error=?err, "failed to subscribe to media OSS queue");
        tokio::time::sleep(Duration::from_secs(10)).await;
        continue;
      }
    };
    while let Some(result) = messages.next().await {
      let message = match result {
        Ok(message) => message,
        Err(err) => {
          warn!(error=?err, "media OSS queue error");
          break;
        }
      };
      let request = match serde_json::from_slice::<TracedMessage<String>>(&message.payload) {
        Ok(request) => request,
        Err(err) => {
          warn!(error=?err, "invalid media OSS request");
          message.double_ack().await.ok();
          continue;
        }
      };
      let sync = sync_one(&state, &request.payload);
      tokio::pin!(sync);
      let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
      let result = loop {
        tokio::select! {
          result = &mut sync => break result,
          _ = heartbeat.tick() => {
            if let Err(err) = message.ack_with(AckKind::Progress).await {
              warn!(error=?err, "failed to extend media OSS acknowledgement");
            }
          }
        }
      };
      match result {
        Ok(()) => {
          message.double_ack().await.ok();
        }
        Err(err) => {
          warn!(hash=%request.payload, error=?err, "media OSS sync failed");
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

pub async fn reconcile_worker(state: GlobalState) {
  let mut ticker = tokio::time::interval(Duration::from_secs(24 * 60 * 60));
  loop {
    ticker.tick().await;
    let mut last_id = 0;
    loop {
      let batch = match media::Entity::find()
        .filter(media::Column::Id.gt(last_id))
        .order_by_asc(media::Column::Id)
        .limit(100)
        .all(&state.db.conn)
        .await
      {
        Ok(batch) => batch,
        Err(err) => {
          warn!(error=?err, "failed to load media for OSS reconciliation");
          break;
        }
      };
      if batch.is_empty() {
        break;
      }
      for item in batch {
        last_id = item.id;
        if let Err(err) = sync_one(&state, &item.hash).await {
          warn!(hash=%item.hash, error=?err, "failed to reconcile media to OSS");
        }
      }
    }
  }
}
