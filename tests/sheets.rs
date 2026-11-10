//! A subject's sheet, rendered from its facts and never stored (docs/design.md, 「A person is a subject」;
//! docs/boundary-api.md, 「Subjects」 and 「Splitting sheets」): the rendering and its room, the refusal of the
//! type `sheet`, the head of it recall's core carries (docs/boundary-api.md, 「Recall」), and the split of the
//! sheet rows a store still holds live, run through the binary.

use crate::support;

use std::path::PathBuf;
use std::process::Output;
use std::sync::atomic::{AtomicU32, Ordering};

use bmo_brain_client::{
  BrainClient, ExportQuery, Learning, MemoriesQuery, MemoryMerge, MemoryMove, MemoryStatus, MemoryVersion,
  NewBrain, NewMemory, NewSubject, RecallAsk, RecallPart, Restoration, SheetQuery, Source, SubjectKey,
};
use support::{Served, TOKEN, labels, refused};
use tokio::process::Command;

fn subject(kind: &str, key: &str) -> SubjectKey {
  SubjectKey {
    kind: kind.to_owned(),
    key: key.to_owned(),
  }
}

fn memory(kind: &str, text: &str) -> NewMemory {
  NewMemory {
    key: None,
    kind: kind.to_owned(),
    text: text.to_owned(),
    layer_id: None,
    position: None,
    subject_ids: vec![],
    subjects: vec![],
    importance: None,
    valid_from: None,
    recall_condition: None,
    release_condition: None,
    status: None,
    actor_name: "writer-a".to_owned(),
    labels: labels(&[]),
  }
}

async fn brain_of(client: &BrainClient, agent: &str) -> i64 {
  client
    .brain_add(&NewBrain {
      account_key: "account-a".to_owned(),
      agent_key: agent.to_owned(),
      name: None,
    })
    .await
    .unwrap()
    .id
}

async fn subject_id(client: &BrainClient, brain: i64, kind: &str, key: &str) -> i64 {
  client
    .subject_add(
      brain,
      &NewSubject {
        kind: kind.to_owned(),
        key: key.to_owned(),
        description: String::new(),
      },
    )
    .await
    .unwrap()
    .id
}

/// The start of a day in UTC, as epoch milliseconds.
fn day(text: &str) -> i64 {
  chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
    .unwrap()
    .and_hms_opt(0, 0, 0)
    .unwrap()
    .and_utc()
    .timestamp_millis()
}

/// A live `sheet` row put in the table as an older store holds one: no route writes one.
async fn put_sheet(served: &Served, brain: i64, subjects: &[i64], text: &str, status: &str) -> i64 {
  let id: i64 = sqlx::query_scalar(
    "INSERT INTO memories (brain_id, type, text, status, lineage_id, valid_from, importance, actor_name, labels, \
     created_at) VALUES ($1, 'sheet', $2, $3, 0, $4, 6, 'writer-a', $5, $4) RETURNING id",
  )
  .bind(brain)
  .bind(text)
  .bind(status)
  .bind(day("2026-01-01"))
  .bind(sqlx::types::Json(labels(&[("origin", "test")])))
  .fetch_one(&served.store.pool)
  .await
  .unwrap();
  sqlx::query("UPDATE memories SET lineage_id = id WHERE id = $1")
    .bind(id)
    .execute(&served.store.pool)
    .await
    .unwrap();
  for subject in subjects {
    sqlx::query("INSERT INTO memory_subjects (memory_id, subject_id) VALUES ($1, $2)")
      .bind(id)
      .bind(subject)
      .execute(&served.store.pool)
      .await
      .unwrap();
  }
  id
}

/// A configuration naming the test's own database, in a folder of the test's own.
struct Workspace {
  folder: PathBuf,
  config: PathBuf,
}

impl Workspace {
  fn over(served: &Served) -> Workspace {
    static MADE: AtomicU32 = AtomicU32::new(0);
    let folder = std::env::temp_dir().join(format!(
      "bmo-brain-sheets-{}-{}",
      std::process::id(),
      MADE.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&folder).unwrap();
    let config = folder.join("brain.toml");
    std::fs::write(
      &config,
      format!("listen = \"[::1]:1\"\ntoken = \"{TOKEN}\"\ndatabase = \"{}\"\n", served.address()),
    )
    .unwrap();
    Workspace { folder, config }
  }

  async fn run(&self, words: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bmo-brain"))
      .arg("--config")
      .arg(&self.config)
      .args(words)
      .kill_on_drop(true)
      .output()
      .await
      .unwrap()
  }
}

impl Drop for Workspace {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.folder);
  }
}

#[tokio::test]
async fn a_sheet_renders_the_live_facts_in_a_fixed_order_and_the_same_bytes_twice() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  let person = subject("person", "1");
  let room = subject("room", "7");
  let write = |kind: &'static str, text: &'static str, subjects: Vec<SubjectKey>, importance, at: &str| {
    let body = NewMemory {
      subjects,
      importance: Some(importance),
      valid_from: Some(day(at)),
      ..memory(kind, text)
    };
    async move { client.memory_add(brain, &body).await.unwrap() }
  };
  let older = write("fact", "Keeps a garden.", vec![person.clone()], 5, "2026-02-01").await;
  let newer = write("fact", "Grows beans.", vec![person.clone()], 5, "2026-03-01").await;
  let weighty = write("fact", "Owns the plot.", vec![person.clone()], 9, "2026-04-01").await;
  let in_room =
    write("fact", "Asks about rain here.", vec![person.clone(), room.clone()], 7, "2026-01-15").await;
  write(
    "fact",
    "Sells seedlings there.",
    vec![person.clone(), subject("room", "8")],
    8,
    "2026-01-15",
  )
  .await;
  write("rumor", "Might move away.", vec![person.clone()], 9, "2026-01-15").await;
  write("tip", "Water at dawn.", vec![person.clone()], 9, "2026-01-15").await;
  write("fact", "Someone else's fact.", vec![subject("person", "2")], 9, "2026-01-15").await;
  let closed = write("fact", "Rents the plot.", vec![person.clone()], 9, "2026-01-15").await;
  client
    .memory_move(
      brain,
      closed.id,
      &MemoryMove {
        status: MemoryStatus::Invalidated,
        at: None,
        actor_name: "writer-a".to_owned(),
        reason: None,
        text: None,
      },
    )
    .await
    .unwrap();
  let person_id = subject_id(client, brain, "person", "1").await;
  let room_id = subject_id(client, brain, "room", "7").await;

  let common = client.sheet(brain, person_id, &SheetQuery::default()).await.unwrap();
  assert_eq!(
    common,
    format!(
      "- [#{} since 2026-04-01] Owns the plot.\n- [#{} since 2026-03-01] Grows beans.\n- [#{} since 2026-02-01] \
       Keeps a garden.",
      weighty.id, newer.id, older.id
    )
  );
  assert_eq!(client.sheet(brain, person_id, &SheetQuery::default()).await.unwrap(), common);

  let roomed = SheetQuery {
    room_id: Some(room_id),
    ..SheetQuery::default()
  };
  let in_the_room = client.sheet(brain, person_id, &roomed).await.unwrap();
  let ids: Vec<i64> = in_the_room
    .lines()
    .map(|line| line.trim_start_matches("- [#").split(' ').next().unwrap().parse().unwrap())
    .collect();
  assert_eq!(ids, [weighty.id, in_room.id, newer.id, older.id]);
  assert_eq!(client.sheet(brain, person_id, &roomed).await.unwrap(), in_the_room);

  let quiet = subject_id(client, brain, "person", "3").await;
  assert_eq!(client.sheet(brain, quiet, &SheetQuery::default()).await.unwrap(), "");
  assert_eq!(
    refused(
      client
        .sheet(
          brain,
          person_id,
          &SheetQuery {
            room_id: Some(person_id),
            ..SheetQuery::default()
          }
        )
        .await
    ),
    (400, "bad-request".to_owned())
  );
  assert_eq!(
    refused(
      client
        .sheet(
          brain,
          person_id,
          &SheetQuery {
            room_id: Some(999_999),
            ..SheetQuery::default()
          }
        )
        .await
    ),
    (404, "not-found".to_owned())
  );
  assert_eq!(
    refused(client.sheet(brain, 999_999, &SheetQuery::default()).await),
    (404, "not-found".to_owned())
  );
}

/// A fact's last confirmation set as a confirmation would set it.
async fn confirm(served: &Served, id: i64, at: i64) {
  sqlx::query("UPDATE memories SET last_confirmed_at = $2 WHERE id = $1")
    .bind(id)
    .bind(at)
    .execute(&served.store.pool)
    .await
    .unwrap();
}

/// The ids of a sheet's lines, in their order.
fn line_ids(sheet: &str) -> Vec<i64> {
  sheet
    .lines()
    .map(|line| line.trim_start_matches("- [#").split(' ').next().unwrap().parse().unwrap())
    .collect()
}

#[tokio::test]
async fn a_sheet_orders_equal_importance_newest_first_and_renders_its_first_lines_on_a_limit() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  let person = subject("person", "1");
  let write = |text: &'static str, importance, at: &str| {
    let body = NewMemory {
      subjects: vec![person.clone()],
      importance: Some(importance),
      valid_from: Some(day(at)),
      ..memory("fact", text)
    };
    async move { client.memory_add(brain, &body).await.unwrap().id }
  };
  let weighty = write("Owns the plot.", 9, "2026-01-01").await;
  let unconfirmed = write("Grows beans.", 5, "2026-03-01").await;
  let confirmed = write("Grows peas.", 5, "2026-03-01").await;
  let confirmed_later = write("Grows leeks.", 5, "2026-03-01").await;
  let unconfirmed_later = write("Grows kale.", 5, "2026-03-01").await;
  let newest = write("Grows squash.", 5, "2026-04-01").await;
  confirm(&served, confirmed, day("2026-05-01")).await;
  confirm(&served, confirmed_later, day("2026-06-01")).await;
  let person_id = subject_id(client, brain, "person", "1").await;

  let whole = client.sheet(brain, person_id, &SheetQuery::default()).await.unwrap();
  assert_eq!(
    line_ids(&whole),
    [weighty, newest, confirmed_later, confirmed, unconfirmed_later, unconfirmed]
  );
  let limited = |limit| SheetQuery {
    limit: Some(limit),
    ..SheetQuery::default()
  };
  let head = client.sheet(brain, person_id, &limited(3)).await.unwrap();
  assert_eq!(head, whole.lines().take(3).collect::<Vec<_>>().join("\n"));
  assert_eq!(client.sheet(brain, person_id, &limited(3)).await.unwrap(), head);
  assert_eq!(client.sheet(brain, person_id, &limited(50)).await.unwrap(), whole);
  assert_eq!(
    refused(client.sheet(brain, person_id, &limited(0)).await),
    (400, "bad-request".to_owned())
  );
}

#[tokio::test]
async fn recall_core_carries_the_cap_of_a_speakers_facts_newest_first_and_keeps_the_budget() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  let room = subject("room", "7");
  let speaker = subject("person", "1");
  let cap = served.store.rules.core_fact_cap;
  let many = cap + 12;
  let mut facts = Vec::new();
  for n in 0..many {
    let body = NewMemory {
      subjects: vec![speaker.clone()],
      importance: Some(5),
      valid_from: Some(day("2026-01-01") + n * 86_400_000),
      ..memory("fact", &format!("Remembered detail number {n}."))
    };
    facts.push(client.memory_add(brain, &body).await.unwrap().id);
  }
  // The oldest of all, past the cap, holding the turn's word.
  let old_tomatoes = client
    .memory_add(
      brain,
      &NewMemory {
        subjects: vec![speaker.clone()],
        importance: Some(5),
        valid_from: Some(day("2025-06-01")),
        ..memory("fact", "Grows tomatoes every summer.")
      },
    )
    .await
    .unwrap()
    .id;
  let ask = |budget: Option<i64>| RecallAsk {
    room: Some(room.clone()),
    speakers: vec![speaker.clone()],
    words: "how are the tomatoes".to_owned(),
    session_key: Some("session-a".to_owned()),
    session_opened_at: None,
    budget_tokens: budget,
    record: None,
  };

  let recall = client.recall(brain, &ask(None)).await.unwrap();
  let core: Vec<i64> = recall
    .chosen
    .iter()
    .filter(|c| c.part == RecallPart::Core)
    .map(|c| c.memory_id)
    .collect();
  let newest_first: Vec<i64> = facts.iter().rev().take(cap as usize).copied().collect();
  assert_eq!(core, newest_first);
  assert!(
    recall
      .chosen
      .iter()
      .any(|c| c.memory_id == old_tomatoes && c.part == RecallPart::Searched),
    "a fact past the cap reaches the turn through the searched part: {:?}",
    recall.chosen
  );
  assert!(recall.token_count <= recall.budget_tokens);
  assert_eq!(recall.token_count, recall.chosen.iter().map(|c| c.token_count).sum::<i64>());

  // A tight budget, then the same session again with its rows marked: neither runs past it.
  for _ in 0..2 {
    let tight = client.recall(brain, &ask(Some(40))).await.unwrap();
    assert!(tight.token_count <= 40, "{} tokens", tight.token_count);
    assert!(!tight.chosen.is_empty());
  }
}

#[tokio::test]
async fn no_write_takes_the_type_sheet() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  let bad = (400, "bad-request".to_owned());
  assert_eq!(refused(client.memory_add(brain, &memory("sheet", "A whole sheet.")).await), bad);
  let one = client.memory_add(brain, &memory("fact", "One.")).await.unwrap();
  let two = client.memory_add(brain, &memory("fact", "Two.")).await.unwrap();
  let as_sheet = MemoryVersion {
    kind: Some("sheet".to_owned()),
    actor_name: "writer-a".to_owned(),
    ..MemoryVersion::default()
  };
  assert_eq!(refused(client.memory_version(brain, one.id, &as_sheet).await), bad);
  let merged = MemoryMerge {
    memory_ids: vec![one.id, two.id],
    kind: "sheet".to_owned(),
    text: "One and two.".to_owned(),
    actor_name: "writer-a".to_owned(),
    ..MemoryMerge::default()
  };
  assert_eq!(refused(client.memory_merge(brain, &merged).await), bad);
  let learned = Learning {
    kind: "sheet".to_owned(),
    text: "A whole sheet.".to_owned(),
    subjects: vec![subject("person", "1")],
    importance: None,
    source: Source::Own,
    confirms: vec![],
    strengthens: None,
    replaces: vec![],
    grounds: vec![],
    valid_from: None,
    recall_condition: None,
    release_condition: None,
    key: None,
    layer_id: None,
    actor_name: "writer-a".to_owned(),
    labels: labels(&[]),
  };
  assert_eq!(refused(client.learn(brain, &learned).await), bad);

  // A sheet an older store holds: no version keeps it a sheet, and once done it is not restored.
  let sheet = put_sheet(&served, brain, &[], "- A line.", "active").await;
  let reworded = MemoryVersion {
    text: Some("- Another line.".to_owned()),
    actor_name: "writer-a".to_owned(),
    ..MemoryVersion::default()
  };
  assert_eq!(refused(client.memory_version(brain, sheet, &reworded).await), bad);
  let file = client.export(brain, &ExportQuery::default()).await.unwrap();
  let other = brain_of(client, "agent-b").await;
  assert_eq!(refused(client.inject(other, file).await), bad);
  client
    .memory_move(
      brain,
      sheet,
      &MemoryMove {
        status: MemoryStatus::Done,
        at: None,
        actor_name: "writer-a".to_owned(),
        reason: None,
        text: None,
      },
    )
    .await
    .unwrap();
  let restored = Restoration {
    text: None,
    actor_name: "writer-a".to_owned(),
  };
  assert_eq!(
    refused(client.memory_restore(brain, sheet, &restored).await),
    (409, "conflict".to_owned())
  );
  assert!(!client.types(brain).await.unwrap().iter().any(|t| t.kind == "sheet" && t.reserved));
}

#[tokio::test]
async fn a_learning_is_dated_where_it_names_when_it_began() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  let learned = client
    .learn(
      brain,
      &Learning {
        kind: "fact".to_owned(),
        text: "Moved to the coast.".to_owned(),
        subjects: vec![subject("person", "1")],
        importance: None,
        source: Source::Own,
        confirms: vec![],
        strengthens: None,
        replaces: vec![],
        grounds: vec![],
        valid_from: Some(day("2026-05-04")),
        recall_condition: None,
        release_condition: None,
        key: None,
        layer_id: None,
        actor_name: "writer-a".to_owned(),
        labels: labels(&[]),
      },
    )
    .await
    .unwrap();
  assert_eq!(learned.written.unwrap().valid_from, day("2026-05-04"));
}

const SHEET_TEXT: &str = "# Profile\n\
\n\
- [date 2026-02-03 · confirmed 2026-03-04] Keeps a garden.\n\
- [fact · confirmed 2026-03-05] Grows beans.\n\
- [basis 2025-01-01~2025-06-30 · confirmed 2026-03-06] Lived by the river.\n\
## Habits\n\
- Walks at dawn.\n\
* [2026-04-01] Reads on trains.\n\
1. [note] Keeps a bracket that is no note.\n\
- [date 2026-02-03]\n\
-\n";

#[tokio::test]
async fn sheets_split_into_facts_once_and_print_ids_and_counts_alone() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client, "agent-a").await;
  let person = subject_id(client, brain, "person", "1").await;
  let room = subject_id(client, brain, "room", "7").await;
  let common = put_sheet(&served, brain, &[person], SHEET_TEXT, "active").await;
  let roomed =
    put_sheet(&served, brain, &[person, room], "## In the room\n- Asks about rain.\n", "pending").await;
  let workspace = Workspace::over(&served);

  let dry = workspace.run(&["sheets", "split", "--dry-run"]).await;
  assert!(dry.status.success(), "{}", String::from_utf8_lossy(&dry.stderr));
  let printed = String::from_utf8(dry.stdout).unwrap();
  assert_eq!(
    printed,
    format!(
      "sheet {common} brain {brain} subjects {person} lines 11 facts 6 headings 2 empty 3\n\
       sheet {roomed} brain {brain} subjects {person},{room} lines 2 facts 1 headings 1 empty 0\n\
       brain {brain} sheets 2 facts 7 skipped 6\n"
    )
  );
  for words in ["garden", "beans", "river", "Asks", "Profile"] {
    assert!(!printed.contains(words), "the output holds no text: {printed}");
  }
  let facts = MemoriesQuery {
    kind: Some("fact".to_owned()),
    ..MemoriesQuery::default()
  };
  assert!(
    client.memories(brain, &facts).await.unwrap().items.is_empty(),
    "a dry run writes nothing"
  );

  let real = workspace.run(&["sheets", "split"]).await;
  assert!(real.status.success(), "{}", String::from_utf8_lossy(&real.stderr));
  assert_eq!(String::from_utf8(real.stdout).unwrap(), printed);

  let all_facts = MemoriesQuery {
    kind: Some("fact".to_owned()),
    status: Some("active,pending".to_owned()),
    ..MemoriesQuery::default()
  };
  let written = client.memories(brain, &all_facts).await.unwrap().items;
  let texts: Vec<&str> = written.iter().map(|m| m.text.as_str()).collect();
  assert_eq!(
    texts,
    [
      "Keeps a garden.",
      "Grows beans.",
      "[2025-01-01~2025-06-30] Lived by the river.",
      "Walks at dawn.",
      "Reads on trains.",
      "[note] Keeps a bracket that is no note.",
      "Asks about rain.",
    ]
  );
  let garden = &written[0];
  assert_eq!(garden.valid_from, day("2026-02-03"));
  assert_eq!(garden.last_confirmed_at, Some(day("2026-03-04")));
  assert_eq!(garden.actor_name, "migration");
  assert_eq!(garden.importance, 6);
  assert_eq!(garden.status, MemoryStatus::Active);
  assert_eq!(garden.subjects.iter().map(|s| s.id).collect::<Vec<_>>(), [person]);
  assert_eq!(garden.labels.get("splitFrom"), Some(&common.to_string()));
  assert_eq!(garden.labels.get("section").map(String::as_str), Some("Profile"));
  assert_eq!(garden.labels.get("origin").map(String::as_str), Some("test"));
  assert_eq!(
    garden.labels.get("prefix").map(String::as_str),
    Some("[date 2026-02-03 · confirmed 2026-03-04]")
  );
  assert_eq!(written[1].valid_from, day("2026-01-01"), "no date read: the sheet's");
  assert_eq!(written[1].last_confirmed_at, Some(day("2026-03-05")));
  assert_eq!(written[2].valid_from, day("2025-01-01"));
  assert_eq!(written[3].labels.get("section").map(String::as_str), Some("Habits"));
  assert_eq!(written[3].labels.get("prefix"), None);
  assert_eq!(written[4].valid_from, day("2026-04-01"));
  assert_eq!(written[5].valid_from, day("2026-01-01"));
  assert_eq!(written[6].status, MemoryStatus::Pending);
  assert_eq!(written[6].subjects.iter().map(|s| s.id).collect::<Vec<_>>(), [person, room]);

  for sheet in [common, roomed] {
    let moved = client.memory(brain, sheet).await.unwrap();
    assert_eq!(moved.status, MemoryStatus::Done);
    assert_eq!(moved.reason.as_deref(), Some("split into facts"));
    assert_eq!(moved.superseded_by, None);
    assert_eq!(moved.actor_name, "writer-a");
  }

  let again = workspace.run(&["sheets", "split"]).await;
  assert!(again.status.success());
  assert_eq!(String::from_utf8(again.stdout).unwrap(), "", "a sheet already split is not read again");
  assert_eq!(client.memories(brain, &all_facts).await.unwrap().items.len(), 7);
}
