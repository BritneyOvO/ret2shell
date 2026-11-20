use chrono::{DateTime, Utc, serde::ts_seconds};
use sea_orm::{IntoActiveModel, entity::prelude::*, sea_query::OnConflict};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "game_writeup_example")]
pub struct Model {
  #[sea_orm(primary_key)]
  pub game_id: i64,
  pub uploader_id: i64,
  pub media_hash: String,
  #[serde(skip)]
  pub storage_key: Option<String>,
  pub filename: String,
  pub content_type: String,
  #[serde(with = "ts_seconds")]
  pub updated_at: DateTime<Utc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
  #[sea_orm(
    belongs_to = "super::game::Entity",
    from = "Column::GameId",
    to = "super::game::Column::Id",
    on_update = "Cascade",
    on_delete = "Cascade"
  )]
  Game,
}

impl Related<super::game::Entity> for Entity {
  fn to() -> RelationDef {
    Relation::Game.def()
  }
}

impl ActiveModelBehavior for ActiveModel {}

pub async fn get<C>(db: &C, game_id: i64) -> Result<Option<Model>, DbErr>
where
  C: ConnectionTrait, {
  Entity::find_by_id(game_id).one(db).await
}

pub async fn upsert<C>(db: &C, model: Model) -> Result<Model, DbErr>
where
  C: ConnectionTrait, {
  Entity::insert(ActiveModel {
    ..model.into_active_model().reset_all()
  })
  .on_conflict(
    OnConflict::column(Column::GameId)
      .update_columns([
        Column::UploaderId,
        Column::MediaHash,
        Column::StorageKey,
        Column::Filename,
        Column::ContentType,
        Column::UpdatedAt,
      ])
      .to_owned(),
  )
  .exec_with_returning(db)
  .await
}
