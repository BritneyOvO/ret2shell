use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
  fn name(&self) -> &str {
    "m_20261119_000004_writeup_latest_per_team"
  }
}

#[derive(Iden)]
enum GameWriteup {
  Table,
  GameId,
  TeamId,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .get_connection()
      .execute_unprepared(
        r#"DELETE FROM game_writeup WHERE id IN (
          SELECT id FROM (
            SELECT id, row_number() OVER (
              PARTITION BY game_id, team_id ORDER BY created_at DESC, id DESC
            ) AS position FROM game_writeup
          ) AS ranked WHERE position > 1
        )"#,
      )
      .await?;
    manager
      .create_index(
        Index::create()
          .name("idx_game_writeup_game_team_unique")
          .table(GameWriteup::Table)
          .col(GameWriteup::GameId)
          .col(GameWriteup::TeamId)
          .unique()
          .to_owned(),
      )
      .await
  }

  async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .drop_index(
        Index::drop()
          .name("idx_game_writeup_game_team_unique")
          .to_owned(),
      )
      .await
  }
}
