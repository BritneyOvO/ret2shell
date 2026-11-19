use sea_orm_migration::prelude::*;

pub struct Migration;

#[derive(Iden)]
enum GameWriteupExample {
  Table,
  GameId,
  UploaderId,
  MediaHash,
  Filename,
  ContentType,
  UpdatedAt,
}

#[derive(Iden)]
enum Game {
  Table,
  Id,
}

impl MigrationName for Migration {
  fn name(&self) -> &str {
    "m_20261120_000001_create_game_writeup_example"
  }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .create_table(
        Table::create()
          .table(GameWriteupExample::Table)
          .if_not_exists()
          .col(
            ColumnDef::new(GameWriteupExample::GameId)
              .big_integer()
              .not_null()
              .primary_key(),
          )
          .col(
            ColumnDef::new(GameWriteupExample::UploaderId)
              .big_integer()
              .not_null(),
          )
          .col(
            ColumnDef::new(GameWriteupExample::MediaHash)
              .string()
              .not_null(),
          )
          .col(
            ColumnDef::new(GameWriteupExample::Filename)
              .string()
              .not_null(),
          )
          .col(
            ColumnDef::new(GameWriteupExample::ContentType)
              .string()
              .not_null(),
          )
          .col(
            ColumnDef::new(GameWriteupExample::UpdatedAt)
              .timestamp_with_time_zone()
              .not_null()
              .default(Expr::current_timestamp()),
          )
          .foreign_key(
            ForeignKey::create()
              .name("fk-game-writeup-example-game")
              .from(GameWriteupExample::Table, GameWriteupExample::GameId)
              .to(Game::Table, Game::Id)
              .on_delete(ForeignKeyAction::Cascade),
          )
          .to_owned(),
      )
      .await
  }

  async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .drop_table(Table::drop().table(GameWriteupExample::Table).to_owned())
      .await
  }
}
