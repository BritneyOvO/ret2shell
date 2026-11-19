use sea_orm_migration::prelude::*;

pub struct Migration;

#[derive(Iden)]
enum GameWriteup {
  Table,
  Id,
  GameId,
  TeamId,
  UserId,
  MediaHash,
  Filename,
  ContentType,
  CreatedAt,
}

#[derive(Iden)]
enum Game {
  Table,
  Id,
}

#[derive(Iden)]
enum Team {
  Table,
  Id,
}

impl MigrationName for Migration {
  fn name(&self) -> &str {
    "m_20261119_000002_create_game_writeup"
  }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .create_table(
        Table::create()
          .table(GameWriteup::Table)
          .if_not_exists()
          .col(
            ColumnDef::new(GameWriteup::Id)
              .big_integer()
              .not_null()
              .auto_increment()
              .primary_key(),
          )
          .col(ColumnDef::new(GameWriteup::GameId).big_integer().not_null())
          .col(ColumnDef::new(GameWriteup::TeamId).big_integer().not_null())
          .col(ColumnDef::new(GameWriteup::UserId).big_integer().not_null())
          .col(ColumnDef::new(GameWriteup::MediaHash).string().not_null())
          .col(ColumnDef::new(GameWriteup::Filename).string().not_null())
          .col(ColumnDef::new(GameWriteup::ContentType).string().not_null())
          .col(
            ColumnDef::new(GameWriteup::CreatedAt)
              .timestamp_with_time_zone()
              .not_null()
              .default(Expr::current_timestamp()),
          )
          .foreign_key(
            ForeignKey::create()
              .name("fk-game-writeup-game")
              .from(GameWriteup::Table, GameWriteup::GameId)
              .to(Game::Table, Game::Id)
              .on_delete(ForeignKeyAction::Cascade),
          )
          .foreign_key(
            ForeignKey::create()
              .name("fk-game-writeup-team")
              .from(GameWriteup::Table, GameWriteup::TeamId)
              .to(Team::Table, Team::Id)
              .on_delete(ForeignKeyAction::Cascade),
          )
          .to_owned(),
      )
      .await
  }

  async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .drop_table(Table::drop().table(GameWriteup::Table).to_owned())
      .await
  }
}
