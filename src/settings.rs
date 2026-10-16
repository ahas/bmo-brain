//! The brain's settings rows (`docs/boundary-api.md`, 「Tables the brain owns」): one row a key, its value
//! text. Each kind of key is built beside the code that writes it.

use brain_support::database::{Fault, all, one, run};
use sea_orm::ConnectionTrait;

/// The value a key holds, or none where no row holds it.
pub async fn get(db: &impl ConnectionTrait, key: &str) -> Result<Option<String>, Fault> {
  let row = one(db, r#"SELECT "value" FROM "settings" WHERE "key" = $1"#, vec![key.into()]).await?;

  Ok(row.map(|row| row.try_get::<String>("", "value")).transpose()?)
}

/// The key set to the value, whatever it held before.
pub async fn set(db: &impl ConnectionTrait, key: &str, value: &str) -> Result<(), Fault> {
  run(
    db,
    r#"INSERT INTO "settings" ("key", "value") VALUES ($1, $2)
       ON CONFLICT ("key") DO UPDATE SET "value" = excluded."value""#,
    vec![key.into(), value.into()],
  )
  .await?;

  Ok(())
}

/// The key's row gone, where there was one.
pub async fn forget(db: &impl ConnectionTrait, key: &str) -> Result<(), Fault> {
  run(db, r#"DELETE FROM "settings" WHERE "key" = $1"#, vec![key.into()]).await?;

  Ok(())
}

/// Every key that carries the prefix, with the rest of the key after it and its value, in key order.
pub async fn prefixed(db: &impl ConnectionTrait, prefix: &str) -> Result<Vec<(String, String)>, Fault> {
  let pattern = format!("{}%", prefix.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
  let rows = all(
    db,
    r#"SELECT "key", "value" FROM "settings" WHERE "key" LIKE $1 ESCAPE '\' ORDER BY "key""#,
    vec![pattern.into()],
  )
  .await?;
  let mut found = Vec::new();

  for row in rows {
    let key = row.try_get::<String>("", "key")?;

    if let Some(rest) = key.strip_prefix(prefix) {
      found.push((rest.to_owned(), row.try_get::<String>("", "value")?));
    }
  }

  Ok(found)
}
