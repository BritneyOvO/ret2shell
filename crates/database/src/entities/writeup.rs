use chrono::{DateTime, Utc, serde::ts_seconds};
use sea_orm::{
  ActiveValue, IntoActiveModel, QueryFilter, QueryOrder, entity::prelude::*, sea_query::OnConflict,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "game_writeup")]
pub struct Model {
  #[sea_orm(primary_key)]
  pub id: i64,
  pub game_id: i64,
  pub team_id: i64,
  pub user_id: i64,
  pub media_hash: String,
  #[serde(skip)]
  pub storage_key: Option<String>,
  pub filename: String,
  pub content_type: String,
  #[serde(with = "ts_seconds")]
  pub created_at: DateTime<Utc>,
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
  #[sea_orm(
    belongs_to = "super::team::Entity",
    from = "Column::TeamId",
    to = "super::team::Column::Id",
    on_update = "Cascade",
    on_delete = "Cascade"
  )]
  Team,
}

impl Related<super::game::Entity> for Entity {
  fn to() -> RelationDef {
    Relation::Game.def()
  }
}

impl Related<super::team::Entity> for Entity {
  fn to() -> RelationDef {
    Relation::Team.def()
  }
}

impl ActiveModelBehavior for ActiveModel {}

pub async fn create<C>(db: &C, model: Model) -> Result<Model, DbErr>
where
  C: ConnectionTrait, {
  Entity::insert(ActiveModel {
    id: ActiveValue::NotSet,
    ..model.into_active_model().reset_all()
  })
  .on_conflict(
    OnConflict::columns([Column::GameId, Column::TeamId])
      .update_columns([
        Column::UserId,
        Column::MediaHash,
        Column::StorageKey,
        Column::Filename,
        Column::ContentType,
        Column::CreatedAt,
      ])
      .to_owned(),
  )
  .exec_with_returning(db)
  .await
}

pub async fn get<C>(db: &C, id: i64) -> Result<Option<Model>, DbErr>
where
  C: ConnectionTrait, {
  Entity::find_by_id(id).one(db).await
}

pub async fn get_by_game<C>(db: &C, game_id: i64) -> Result<Vec<Model>, DbErr>
where
  C: ConnectionTrait, {
  Entity::find()
    .filter(Column::GameId.eq(game_id))
    .order_by_desc(Column::CreatedAt)
    .all(db)
    .await
}

pub async fn get_by_team<C>(db: &C, game_id: i64, team_id: i64) -> Result<Vec<Model>, DbErr>
where
  C: ConnectionTrait, {
  Entity::find()
    .filter(Column::GameId.eq(game_id))
    .filter(Column::TeamId.eq(team_id))
    .order_by_desc(Column::CreatedAt)
    .all(db)
    .await
}
