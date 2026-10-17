//! The word a screen calls each shipped kind by, as its owner last handed it in: one row a kind, which the archive
//! never held, since its screens named the kinds in their own code.

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
  fn name(&self) -> &str {
    "m20261017_000001_fragment_kinds"
  }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .get_connection()
      .execute_unprepared(
        r#"CREATE TABLE "fragment_kinds" (
             "kind" text NOT NULL,
             "label" text NOT NULL,
             "set_at" bigint NOT NULL,
             PRIMARY KEY ("kind")
           );"#,
      )
      .await?;

    Ok(())
  }

  async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager.get_connection().execute_unprepared(r#"DROP TABLE "fragment_kinds";"#).await?;

    Ok(())
  }
}
