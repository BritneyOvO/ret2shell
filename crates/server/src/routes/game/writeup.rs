use axum::{
  Extension, Json, Router,
  body::Body,
  extract::{DefaultBodyLimit, Multipart, Path, State},
  http::{HeaderValue, StatusCode, header::CONTENT_DISPOSITION},
  middleware,
  response::{IntoResponse, Response},
  routing::get,
};
use futures::TryStreamExt;
use r2s_database::{game, media, team, user::Permission, writeup, writeup_example};
use r2s_media::{Media, PendingDocument};
use r2s_migrator::Database;
use sea_orm::{
  ColumnTrait, DatabaseTransaction, EntityTrait, PaginatorTrait, QueryFilter, QuerySelect,
  TransactionTrait, sea_query::LockType,
};
use tokio_util::io::{ReaderStream, StreamReader};
use tracing::warn;

use crate::{
  middleware::auth::{self, Token, is_game_admin},
  traits::{GlobalState, ResponseError},
};

const MAX_WRITEUP_SIZE: usize = 50 * 1024 * 1024;

pub(crate) async fn get_all_writeups(
  State(ref db): State<Database>, Extension(game): Extension<game::Model>,
) -> Result<impl IntoResponse, ResponseError> {
  Ok(Json(writeup::get_by_game(&db.conn, game.id).await?))
}

pub fn router(_state: &GlobalState) -> Router<GlobalState> {
  Router::new()
    .route("/writeup", get(get_self_writeups).post(submit_writeup))
    .route(
      "/writeup/example",
      get(get_writeup_example).post(upload_writeup_example),
    )
    .route("/writeup/example/download", get(download_writeup_example))
    .route("/writeup/{writeup}/download", get(download_writeup))
    .route_layer(middleware::from_fn(auth::permission_required_all!(
      Permission::Basic,
      Permission::Verified
    )))
    .layer(DefaultBodyLimit::max(MAX_WRITEUP_SIZE))
}

async fn get_writeup_example(
  State(ref db): State<Database>, Extension(game): Extension<game::Model>,
) -> Result<impl IntoResponse, ResponseError> {
  Ok(Json(writeup_example::get(&db.conn, game.id).await?))
}

async fn upload_writeup_example(
  State(ref state): State<GlobalState>, Extension(game): Extension<game::Model>,
  Extension(token): Extension<Token>, mut multipart: Multipart,
) -> Result<impl IntoResponse, ResponseError> {
  if !is_game_admin!(token, game) {
    return Err(ResponseError::Forbidden("permission denied".to_owned()));
  }
  let field = multipart
    .next_field()
    .await
    .map_err(|err| ResponseError::BadRequest(err.to_string()))?
    .ok_or_else(|| ResponseError::BadRequest("no WP example uploaded".to_owned()))?;
  let filename = field
    .file_name()
    .map(str::to_owned)
    .ok_or_else(|| ResponseError::BadRequest("WP example filename is required".to_owned()))?;
  let content_type = validate_file(&filename, field.content_type())?;
  let reader = StreamReader::new(field.map_err(std::io::Error::other));
  let pending = state.media.save_document(reader).await?;
  let state = state.clone();
  let example = tokio::spawn(replace_example(
    state,
    writeup_example::Model {
      game_id: game.id,
      uploader_id: token.id,
      media_hash: pending.hash().to_owned(),
      storage_key: Some(pending.key().to_owned()),
      filename,
      content_type,
      updated_at: chrono::Utc::now(),
    },
    pending,
  ))
  .await
  .map_err(write_task_error)??;
  Ok((StatusCode::CREATED, Json(example)))
}

async fn download_writeup_example(
  State(ref state): State<GlobalState>, Extension(game): Extension<game::Model>,
) -> Result<Response, ResponseError> {
  let txn = lock_game_documents(&state.db, game.id, LockType::Share).await?;
  let example = writeup_example::get(&txn, game.id)
    .await?
    .ok_or_else(|| ResponseError::NotFound("WP example not found".to_owned()))?;
  let file = open_document(
    &state.media,
    example.storage_key.as_deref(),
    &example.media_hash,
  )
  .await?;
  txn.commit().await?;
  let mut response = Response::new(Body::from_stream(ReaderStream::new(file)));
  response.headers_mut().insert(
    "content-type",
    HeaderValue::from_str(&example.content_type)
      .map_err(|_| ResponseError::InternalServerError("invalid content type".to_owned()))?,
  );
  let disposition = format!(
    "attachment; filename*=UTF-8''{}",
    urlencoding::encode(&example.filename)
  );
  response.headers_mut().insert(
    CONTENT_DISPOSITION,
    HeaderValue::from_str(&disposition)
      .map_err(|_| ResponseError::InternalServerError("invalid filename".to_owned()))?,
  );
  Ok(response)
}

async fn get_self_writeups(
  State(ref db): State<Database>, Extension(game): Extension<game::Model>,
  Extension(team): Extension<Option<team::Model>>,
) -> Result<impl IntoResponse, ResponseError> {
  let team = team.ok_or_else(|| ResponseError::NotFound("team not found".to_owned()))?;
  Ok(Json(
    writeup::get_by_team(&db.conn, game.id, team.id).await?,
  ))
}

fn validate_file(filename: &str, content_type: Option<&str>) -> Result<String, ResponseError> {
  let extension = filename
    .rsplit_once('.')
    .map(|(_, extension)| extension.to_ascii_lowercase())
    .ok_or_else(|| ResponseError::BadRequest("WP must be a PDF or Word file".to_owned()))?;
  let expected_type = match extension.as_str() {
    "pdf" => "application/pdf",
    "doc" => "application/msword",
    "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    _ => {
      return Err(ResponseError::BadRequest(
        "WP must be a PDF or Word file".to_owned(),
      ));
    }
  };
  if content_type != Some(expected_type) {
    return Err(ResponseError::BadRequest(format!(
      "{extension} files must use MIME type {expected_type}"
    )));
  }
  Ok(expected_type.to_owned())
}

async fn submit_writeup(
  State(ref state): State<GlobalState>, Extension(game): Extension<game::Model>,
  Extension(team): Extension<Option<team::Model>>, Extension(token): Extension<Token>,
  mut multipart: Multipart,
) -> Result<impl IntoResponse, ResponseError> {
  let team = team.ok_or_else(|| ResponseError::NotFound("team not found".to_owned()))?;
  let field = multipart
    .next_field()
    .await
    .map_err(|err| ResponseError::BadRequest(err.to_string()))?
    .ok_or_else(|| ResponseError::BadRequest("no WP file uploaded".to_owned()))?;
  let filename = field
    .file_name()
    .map(str::to_owned)
    .ok_or_else(|| ResponseError::BadRequest("WP filename is required".to_owned()))?;
  let content_type = validate_file(&filename, field.content_type())?;
  let reader = StreamReader::new(field.map_err(std::io::Error::other));
  let pending = state.media.save_document(reader).await?;
  let state = state.clone();
  let submission = tokio::spawn(replace_submission(
    state,
    writeup::Model {
      id: 0,
      game_id: game.id,
      team_id: team.id,
      user_id: token.id,
      media_hash: pending.hash().to_owned(),
      storage_key: Some(pending.key().to_owned()),
      filename,
      content_type,
      created_at: chrono::Utc::now(),
    },
    pending,
  ))
  .await
  .map_err(write_task_error)??;
  Ok((StatusCode::CREATED, Json(submission)))
}

async fn download_writeup(
  State(ref state): State<GlobalState>, Extension(game): Extension<game::Model>,
  Extension(team): Extension<Option<team::Model>>, Extension(token): Extension<Token>,
  Path((_, writeup_id)): Path<(String, i64)>,
) -> Result<Response, ResponseError> {
  let txn = lock_game_documents(&state.db, game.id, LockType::Share).await?;
  let submission = writeup::get(&txn, writeup_id)
    .await?
    .ok_or_else(|| ResponseError::NotFound("WP not found".to_owned()))?;
  if submission.game_id != game.id {
    return Err(ResponseError::NotFound("WP not found".to_owned()));
  }
  if !is_game_admin!(token, game)
    && team
      .as_ref()
      .is_none_or(|current| current.id != submission.team_id)
  {
    return Err(ResponseError::Forbidden(
      "not allowed to download this WP".to_owned(),
    ));
  }
  let file = open_document(
    &state.media,
    submission.storage_key.as_deref(),
    &submission.media_hash,
  )
  .await?;
  txn.commit().await?;
  let mut response = Response::new(Body::from_stream(ReaderStream::new(file)));
  response.headers_mut().insert(
    "content-type",
    HeaderValue::from_str(&submission.content_type)
      .map_err(|_| ResponseError::InternalServerError("invalid content type".to_owned()))?,
  );
  let disposition = format!(
    "attachment; filename*=UTF-8''{}",
    urlencoding::encode(&submission.filename)
  );
  response.headers_mut().insert(
    CONTENT_DISPOSITION,
    HeaderValue::from_str(&disposition)
      .map_err(|_| ResponseError::InternalServerError("invalid filename".to_owned()))?,
  );
  Ok(response)
}

async fn lock_game_documents(
  db: &Database, game_id: i64, lock: LockType,
) -> Result<DatabaseTransaction, ResponseError> {
  let txn = db.conn.begin().await?;
  game::Entity::find_by_id(game_id)
    .lock(lock)
    .one(&txn)
    .await?
    .ok_or_else(|| ResponseError::NotFound("game not found".to_owned()))?;
  Ok(txn)
}

fn write_task_error(error: tokio::task::JoinError) -> ResponseError {
  warn!(?error, "writeup storage task failed");
  ResponseError::InternalServerError("failed to save WP".to_owned())
}

async fn replace_example(
  state: GlobalState, model: writeup_example::Model, pending: PendingDocument,
) -> Result<writeup_example::Model, ResponseError> {
  let txn = lock_game_documents(&state.db, model.game_id, LockType::Update).await?;
  let previous = writeup_example::get(&txn, model.game_id).await?;
  let result = writeup_example::upsert(&txn, model).await?;
  commit_document(&state, txn, pending).await?;
  if let Some(previous) = previous {
    retire_document(
      &state,
      previous.storage_key.as_deref(),
      &previous.media_hash,
    )
    .await;
  }
  Ok(result)
}

async fn replace_submission(
  state: GlobalState, model: writeup::Model, pending: PendingDocument,
) -> Result<writeup::Model, ResponseError> {
  let txn = lock_game_documents(&state.db, model.game_id, LockType::Update).await?;
  let previous = writeup::get_by_team(&txn, model.game_id, model.team_id)
    .await?
    .into_iter()
    .next();
  let result = writeup::create(&txn, model).await?;
  commit_document(&state, txn, pending).await?;
  if let Some(previous) = previous {
    retire_document(
      &state,
      previous.storage_key.as_deref(),
      &previous.media_hash,
    )
    .await;
  }
  Ok(result)
}

async fn commit_document(
  state: &GlobalState, txn: DatabaseTransaction, pending: PendingDocument,
) -> Result<(), ResponseError> {
  if let Err(error) = txn.commit().await {
    // A lost commit response can leave a live reference to the staged file.
    let key = pending.key();
    let submissions = writeup::Entity::find()
      .filter(writeup::Column::StorageKey.eq(key))
      .count(&state.db.conn)
      .await;
    let examples = writeup_example::Entity::find()
      .filter(writeup_example::Column::StorageKey.eq(key))
      .count(&state.db.conn)
      .await;
    if !matches!((submissions, examples), (Ok(0), Ok(0))) {
      pending.retain();
    }
    return Err(error.into());
  }
  pending.retain();
  Ok(())
}

async fn open_document(
  media: &Media, storage_key: Option<&str>, hash: &str,
) -> Result<tokio::fs::File, r2s_media::MediaError> {
  match storage_key {
    Some(key) => media.get_document(key).await,
    None => media.get(hash).await,
  }
}

async fn retire_document(state: &GlobalState, storage_key: Option<&str>, hash: &str) {
  let result = match storage_key {
    Some(key) => state.media.delete_document(key).await.map_err(Into::into),
    None => retire_legacy_document(state, hash).await,
  };
  if let Err(error) = result {
    warn!(?error, hash, "failed to retire replaced writeup");
  }
}

async fn retire_legacy_document(state: &GlobalState, hash: &str) -> Result<(), ResponseError> {
  let db = &state.db.conn;
  if media::get_by_hash(db, hash)
    .await?
    .is_some_and(|model| !model.private_document)
    || writeup::Entity::find()
      .filter(writeup::Column::StorageKey.is_null())
      .filter(writeup::Column::MediaHash.eq(hash))
      .count(db)
      .await?
      != 0
    || writeup_example::Entity::find()
      .filter(writeup_example::Column::StorageKey.is_null())
      .filter(writeup_example::Column::MediaHash.eq(hash))
      .count(db)
      .await?
      != 0
  {
    return Ok(());
  }
  match state.media.delete(hash).await {
    Ok(()) => Ok(()),
    Err(r2s_media::MediaError::IOError(error)) if error.kind() == std::io::ErrorKind::NotFound => {
      Ok(())
    }
    Err(error) => Err(error.into()),
  }
}
