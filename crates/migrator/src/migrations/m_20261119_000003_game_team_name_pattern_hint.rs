use sea_orm_migration::prelude::*;

pub struct Migration;

#[derive(Iden)]
enum Game {
  Table,
  TeamNamePatternHint,
}

impl MigrationName for Migration {
  fn name(&self) -> &str {
    "m_20261119_000003_game_team_name_pattern_hint"
  }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .alter_table(
        Table::alter()
          .table(Game::Table)
          .add_column_if_not_exists(ColumnDef::new(Game::TeamNamePatternHint).text())
          .to_owned(),
      )
      .await
  }

  async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .alter_table(
        Table::alter()
          .table(Game::Table)
          .drop_column(Game::TeamNamePatternHint)
          .to_owned(),
      )
      .await
  }
}
