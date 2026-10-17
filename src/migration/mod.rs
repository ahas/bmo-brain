//! The brain's migrations on PostgreSQL, run by sea-orm's migrator, which keeps the versions it ran in its
//! own table.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectOptions, Database};

mod m20261016_000001_brain;
mod m20261017_000001_fragment_kinds;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
  fn migrations() -> Vec<Box<dyn MigrationTrait>> {
    vec![
      Box::new(m20261016_000001_brain::Migration),
      Box::new(m20261017_000001_fragment_kinds::Migration),
    ]
  }
}

/// Every migration of this code run up over the database an address names.
pub async fn migrate(address: &str) -> Result<(), DbErr> {
  let mut options = ConnectOptions::new(address.to_owned());

  options.sqlx_logging(false);

  let db = Database::connect(options).await?;

  Migrator::up(&db, None).await?;
  db.close().await?;

  Ok(())
}
