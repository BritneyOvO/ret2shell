use sea_orm_migration::prelude::*;

pub struct Migration;

#[derive(Iden)]
enum Game {
  Table,
  TeamNamePattern,
}

impl MigrationName for Migration {
  fn name(&self) -> &str {
    "m_20261119_000001_game_team_name_pattern"
  }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .alter_table(
        Table::alter()
          .table(Game::Table)
          .add_column_if_not_exists(ColumnDef::new(Game::TeamNamePattern).text())
          .to_owned(),
      )
      .await?;
    Ok(())
  }

  async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .alter_table(
        Table::alter()
          .table(Game::Table)
          .drop_column(Game::TeamNamePattern)
          .to_owned(),
      )
      .await?;
    Ok(())
  }
}
