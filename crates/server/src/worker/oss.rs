use std::{path::Path, time::Duration};

use r2s_database::game;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use tracing::warn;

use crate::traits::GlobalState;

pub async fn sync_worker(state: GlobalState) {
  let Some(oss) = &state.attachment_oss else {
    return;
  };
  let Some(bucket) = &state.config.bucket else {
    return;
  };
  let mut ticker = tokio::time::interval(Duration::from_secs(60));
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
