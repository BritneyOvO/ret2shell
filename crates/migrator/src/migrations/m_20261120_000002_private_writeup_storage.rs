use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
  fn name(&self) -> &str {
    "m_20261120_000002_private_writeup_storage"
  }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .get_connection()
      .execute_unprepared(
        "ALTER TABLE game_writeup ADD COLUMN storage_key TEXT;
         ALTER TABLE game_writeup_example ADD COLUMN storage_key TEXT;
         ALTER TABLE media ADD COLUMN private_document BOOLEAN NOT NULL DEFAULT FALSE;
         UPDATE media SET private_document = TRUE WHERE hash IN (
           SELECT media_hash FROM game_writeup
           UNION
           SELECT media_hash FROM game_writeup_example
         );",
      )
      .await?;
    Ok(())
  }

  async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .get_connection()
      .execute_unprepared(
        "ALTER TABLE game_writeup DROP COLUMN storage_key;
         ALTER TABLE game_writeup_example DROP COLUMN storage_key;
         ALTER TABLE media DROP COLUMN private_document;",
      )
      .await?;
    Ok(())
  }
}
