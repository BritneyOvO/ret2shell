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
use r2s_database::{game, media, team, user::Permission, writeup};
use r2s_migrator::Database;
use tokio_util::io::{ReaderStream, StreamReader};

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
    .route("/writeup/{writeup}/download", get(download_writeup))
    .route_layer(middleware::from_fn(auth::permission_required_all!(
      Permission::Basic,
      Permission::Verified
    )))
    .layer(DefaultBodyLimit::max(MAX_WRITEUP_SIZE))
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
  let stored = state.media.save_document(reader).await?;
  if media::get_by_hash(&state.db.conn, &stored.hash)
    .await?
    .is_none()
  {
    media::create(
      &state.db.conn,
      media::Model {
        uploader_id: token.id,
        ..stored.clone()
      },
    )
    .await?;
  }
  let submission = writeup::create(
    &state.db.conn,
    writeup::Model {
      id: 0,
      game_id: game.id,
      team_id: team.id,
      user_id: token.id,
      media_hash: stored.hash,
      filename,
      content_type,
      created_at: chrono::Utc::now(),
    },
  )
  .await?;
  Ok((StatusCode::CREATED, Json(submission)))
}

async fn download_writeup(
  State(ref state): State<GlobalState>, Extension(game): Extension<game::Model>,
  Extension(team): Extension<Option<team::Model>>, Extension(token): Extension<Token>,
  Path((_, writeup_id, _)): Path<(String, i64, String)>,
) -> Result<Response, ResponseError> {
  let submission = writeup::get(&state.db.conn, writeup_id)
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
  let file = state.media.get(&submission.media_hash).await?;
  let mut response = Response::new(Body::from_stream(ReaderStream::new(file)));
  response.headers_mut().insert(
    "content-type",
    HeaderValue::from_str(&submission.content_type)
      .map_err(|_| ResponseError::InternalServerError("invalid content type".to_owned()))?,
  );
  let disposition = format!(
    "attachment; filename=\"{}\"",
    submission.filename.replace('"', "_")
  );
  response.headers_mut().insert(
    CONTENT_DISPOSITION,
    HeaderValue::from_str(&disposition)
      .map_err(|_| ResponseError::InternalServerError("invalid filename".to_owned()))?,
  );
  Ok(response)
}
