//! The brain's tables as the archive holds them at the move, with their indexes, and the settings table of
//! the archive's shape; the `agent` column names no default agent.

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
  fn name(&self) -> &str {
    "m20261016_000001_brain"
  }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager.get_connection().execute_unprepared(include_str!("brain.sql")).await?;

    Ok(())
  }

  async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
    Err(DbErr::Migration(
      "the brain's first migration is not undone; its database is dropped instead".into(),
    ))
  }
}
