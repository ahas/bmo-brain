//! The fragment table as the editors and a shipped library's owner reach it: which library a brain path is a piece
//! of in whichever agent's folder it stands, the index files drawn out of the rows, the narrowing a writer's
//! pieces pass, a batch's row writes held against its writing and applied, the brain's rows reconciled with what
//! stands on disk, and a shipped library's rows brought to the entries its owner hands in.

mod support;

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bmo_brain::editors::fragments::libraries::{
  AgentFolder, Brain, Knowledge, Library, PersonBook, Said, brain_kind, drawn_index,
};
use bmo_brain::editors::fragments::narrow::{Turn, index_line, items_asked, rows_to_hand};
use bmo_brain::editors::fragments::store::{self, Row, ShippedEntry, Sync, of_kind};
use bmo_brain::editors::fragments::{BatchText, ShippedFailure, Table, Writing, names, write_problems};
use bmo_brain::policy::ITEM_WORDS;
use fragment_desk::desk::{FragmentFilter, FragmentWrite};

use support::agency::{FakeAgency, registration};
use support::receiver::FakeReceiver;
use support::scratch::Scratch;
use support::{AGENT, OTHER_AGENT, ROOM};

const COMPANION: &str = "agents/agent-b";

fn row(
  id: i64,
  kind: &str,
  agent: &str,
  chat_id: Option<i64>,
  owner: Option<i64>,
  slug: &str,
  position: i64,
) -> Row {
  Row {
    id,
    kind: kind.into(),
    chat_id,
    owner,
    agent: agent.into(),
    all_agents: false,
    slug: slug.into(),
    title: format!("about {slug}"),
    path: String::new(),
    scope: None,
    position,
    active: true,
    keywords: vec![],
    items: vec![],
    written_on: Some("2026-01-02".into()),
    confirmed_on: Some("2026-03-04".into()),
    status: "open".into(),
    body_hash: None,
    hand: None,
  }
}

fn brain_at(root: &Path) -> Brain {
  Brain::new(
    root.to_path_buf(),
    AGENT,
    vec![AgentFolder {
      agent: OTHER_AGENT.into(),
      folder: format!("{COMPANION}/"),
      label: Some("Helper".into()),
    }],
  )
}

fn brain() -> Brain {
  brain_at(Path::new("/brain"))
}

fn write(path: &str, title: &str, keywords: &[&str], items: &[i64]) -> FragmentWrite {
  FragmentWrite {
    path: path.into(),
    removed: false,
    title: title.into(),
    keywords: keywords.iter().map(|word| (*word).to_owned()).collect(),
    items: items.to_vec(),
    shared: None,
    all_agents: None,
  }
}

fn entry(slug: &str, title: &str, position: i64) -> ShippedEntry {
  ShippedEntry {
    slug: slug.into(),
    title: title.into(),
    path: format!("docs/{slug}.md"),
    position,
    scope: None,
    items: vec![],
    written_on: Some("2026-01-02".into()),
    confirmed_on: None,
    body_hash: Some(format!("hash-{slug}")),
  }
}

// A brain on disk that is a git repository, holding these files.
fn laid_brain(files: &[(&str, &str)]) -> tempfile::TempDir {
  let dir = tempfile::tempdir().unwrap();

  fs::create_dir_all(dir.path().join(".git")).unwrap();
  for (path, text) in files {
    let file = dir.path().join(path);

    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, text).unwrap();
  }

  dir
}

fn agency() -> FakeAgency {
  let agency = FakeAgency::default();
  let root = registration(AGENT);
  let companion = registration(OTHER_AGENT);

  *agency.registrations.lock().unwrap() = vec![
    bmo_brain::ports::agency::Registration {
      brain_folder: String::new(),
      ..root
    },
    bmo_brain::ports::agency::Registration {
      brain_folder: COMPANION.into(),
      label: Some("Helper".into()),
      ..companion
    },
  ];
  agency
}

#[test]
fn a_piece_is_its_librarys_in_whichever_agents_folder_it_stands() {
  let brain = brain();
  let said = Said {
    shared: None,
    room: Some(5),
    standing: None,
  };

  assert_eq!(brain.root_agent(), AGENT);
  assert_eq!(
    brain
      .library_owning(&format!("{COMPANION}/rooms/-5/rules/greeting.md"))
      .map(|one| one.kind()),
    Some("room-rule")
  );
  assert!(brain.library_owning("rooms/-5/rules/index.md").is_none());
  assert!(brain.library_owning("knowledge/candidates.md").is_none());
  assert!(brain.library_owning("people/7/chunks/c8-address.md").is_none());

  let placed = brain
    .place(&PersonBook, &format!("{COMPANION}/people/7/chunks/c7-address.md"), &said)
    .unwrap();

  assert_eq!(placed.agent.as_deref(), Some(OTHER_AGENT));
  assert_eq!((placed.chat_id, placed.owner, placed.slug.as_str()), (None, Some(7), "c7-address"));
  assert_eq!(
    brain.index_of(&PersonBook, Some(-5), Some(7), OTHER_AGENT),
    format!("{COMPANION}/rooms/-5/people/index.md")
  );
  assert_eq!(brain.index_of(&PersonBook, None, Some(7), AGENT), "people/7/index.md");
}

#[test]
fn only_the_brains_three_libraries_are_its_own() {
  assert!(brain_kind("room-rule") && brain_kind("library-knowledge") && brain_kind("person-book"));
  assert!(!brain_kind("lore") && !brain_kind("system-document"));
}

#[test]
fn a_knowledge_chunk_keeps_the_scope_its_row_has_where_its_line_says_none() {
  let standing = row(1, "library-knowledge", AGENT, None, None, "a", 0);
  let said = |shared, standing| Said {
    shared,
    room: Some(5),
    standing,
  };

  assert_eq!(Knowledge.place("knowledge/a.md", &said(None, Some(&standing))).unwrap().chat_id, None);
  assert_eq!(
    Knowledge.place("knowledge/a.md", &said(None, None)).unwrap().scope.as_deref(),
    Some("[chat:5]")
  );
  assert_eq!(
    Knowledge.place("knowledge/a.md", &said(Some(false), Some(&standing))).unwrap().chat_id,
    Some(5)
  );
  assert!(
    Knowledge
      .place(
        "knowledge/a.md",
        &Said {
          shared: None,
          room: None,
          standing: None
        }
      )
      .is_none()
  );
}

#[test]
fn a_room_index_is_drawn_in_sections_per_person_written_as_its_agent_calls_itself() {
  let brain = brain();
  let rows = vec![
    row(3, "person-book", AGENT, Some(-5), Some(9), "p9-address", 0),
    row(1, "person-book", AGENT, Some(-5), Some(7), "p7-address", 1),
    row(2, "person-book", AGENT, Some(-5), Some(9), "p9-ongoing", 2),
    row(4, "person-book", OTHER_AGENT, Some(-5), Some(9), "p9-address", 0),
  ];
  let names = HashMap::from([(9, Some("Tester".to_owned())), (7, None)]);
  let drawn = brain.draw(&PersonBook, &rows, &names);

  assert_eq!(drawn.len(), 2);
  assert_eq!(drawn[0].0, "rooms/-5/people/index.md");
  assert!(
    drawn[0]
      .1
      .starts_with(&format!("# Person book (room -5)\n\nThe room-only part of what {AGENT} writes"))
  );
  assert!(drawn[0].1.ends_with(
    "## 9 — Tester\n\n- p9-address — about p9-address · 2026-01~2026-03\n- p9-ongoing — about p9-ongoing · \
     2026-01~2026-03\n\n## 7 — unknown\n\n- p7-address — about p7-address · 2026-01~2026-03\n"
  ));
  assert_eq!(drawn[1].0, format!("{COMPANION}/rooms/-5/people/index.md"));
  assert!(drawn[1].1.contains("what Helper writes"));
}

#[test]
fn a_flat_index_lists_its_rows_under_its_header() {
  let rows = [row(1, "library-knowledge", AGENT, None, None, "a", 0)];
  let refs: Vec<&Row> = rows.iter().collect();

  assert_eq!(
    drawn_index("# Knowledge index", &refs, |_| None),
    "# Knowledge index\n\n- a — about a · 2026-01~2026-03\n"
  );
  assert_eq!(
    index_line(&Row {
      scope: Some("[shared]".into()),
      written_on: None,
      confirmed_on: None,
      ..rows[0].clone()
    }),
    "- a [shared] — about a"
  );
}

#[test]
fn an_item_is_named_by_its_mark_its_word_or_as_one() {
  let word = &ITEM_WORDS.after_number;
  let other = &ITEM_WORDS.not_after[0];
  let named = &ITEM_WORDS.before_number;

  assert_eq!(items_asked(&format!("see #758 and 123{word} and 45 {word}{other}")), vec![758, 123]);
  assert_eq!(items_asked(&format!("{named} #77, then 123456{word}")), vec![77]);
  assert_eq!(items_asked("see #758 and 123\u{bc88} and 45 \u{bc88}\u{d638}"), vec![758, 123]);
}

#[test]
fn a_writer_is_handed_what_the_batch_names_beside_what_it_always_gets() {
  let rows = vec![
    Row {
      keywords: vec!["herdr".into()],
      ..row(1, "person-book", AGENT, Some(-5), Some(9), "p9-tools", 0)
    },
    Row {
      items: vec![758],
      ..row(2, "library-knowledge", AGENT, None, None, "restart", 1)
    },
    row(3, "library-knowledge", AGENT, None, None, "kept", 2),
    row(4, "library-knowledge", AGENT, None, None, "unnamed", 3),
  ];
  let turn = Turn {
    text: "the herdr restart of #758".into(),
    speakers: vec![9],
    names: HashMap::new(),
  };
  let handed: Vec<i64> =
    rows_to_hand(&rows, &turn, |row| row.slug == "kept").iter().map(|row| row.id).collect();

  assert_eq!(handed, vec![1, 2, 3]);
}

#[test]
fn a_run_s_row_writes_are_held_against_what_it_wrote() {
  let brain = brain();
  let names = names(&brain, AGENT);
  let writes = vec![
    write("notes/loose.md", "x", &[], &[]),
    write("knowledge/elsewhere.md", "Elsewhere", &[], &[]),
    write("knowledge/missing.md", "Missing", &[], &[]),
    FragmentWrite {
      removed: true,
      ..write("knowledge/gone.md", "", &[], &[])
    },
  ];
  let changed = vec!["knowledge/unwritten.md".to_owned(), "knowledge/gone.md".to_owned()];
  let problems = write_problems(
    &brain,
    &Writing {
      writes: &writes,
      changed: &changed,
      allowed: &|path| path != "knowledge/elsewhere.md",
      body: &|_| None,
      seen: &|path| path.to_owned(),
    },
    &names,
  );

  assert_eq!(
    problems,
    vec![
      "the row write of notes/loose.md names no piece",
      "the row write of knowledge/elsewhere.md names a piece outside where the run may write",
      "the row write of knowledge/missing.md names a piece that is not on disk",
      "knowledge/unwritten.md was written without its row",
      "knowledge/gone.md was written without its row",
    ]
  );
  assert_eq!(names.agent, AGENT);
  assert_eq!(names.agent_key, AGENT);
}

#[tokio::test]
async fn the_table_follows_the_agencys_folders_and_labels() {
  let scratch = Scratch::new().await;
  let agency = agency();
  let receiver = FakeReceiver::default();
  let table = Table::of(scratch.db(), &agency, &receiver, Path::new("/brain"), AGENT).await.unwrap();

  assert_eq!(table.brain.root_agent(), AGENT);
  assert_eq!(table.brain.folder_of(OTHER_AGENT), COMPANION);
  assert_eq!(table.brain.writer_name(OTHER_AGENT), "Helper");
  assert_eq!(table.names.agent_key, AGENT);
}

#[tokio::test]
async fn a_batch_s_writes_become_rows_settle_and_draw_their_agents_index_alone() {
  let scratch = Scratch::new().await;
  let agency = agency();
  let receiver = FakeReceiver::default();
  let dir = laid_brain(&[
    ("knowledge/restart.md", "# Restart\n\nThe restart of the tool.\n"),
    (&format!("{COMPANION}/knowledge/tea.md"), "# Tea\n\nThe tea notes.\n"),
  ]);
  let table = Table::of(scratch.db(), &agency, &receiver, dir.path(), AGENT).await.unwrap();

  agency.finished.lock().unwrap().insert(758);
  table
    .follow_write(
      &[FragmentWrite {
        shared: Some(true),
        ..write("knowledge/restart.md", "Restarting the tool", &["restart"], &[758])
      }],
      Some(ROOM),
      AGENT,
    )
    .await
    .unwrap();

  let rows = table.rows(&of_kind("library-knowledge")).await.unwrap();

  assert_eq!(rows.len(), 1);
  assert_eq!(
    (rows[0].agent.as_str(), rows[0].chat_id, rows[0].scope.as_deref()),
    (AGENT, None, Some("[shared]"))
  );
  assert_eq!(
    (rows[0].items.clone(), rows[0].status.as_str(), rows[0].position),
    (vec![758], "done", 0)
  );
  assert!(rows[0].body_hash.is_some());
  assert_eq!(
    fs::read_to_string(dir.path().join("knowledge/index.md")).unwrap(),
    "# Knowledge index\n\n- restart [shared] — Restarting the tool\n"
  );

  table
    .follow_write(
      &[write(&format!("{COMPANION}/knowledge/tea.md"), "Tea notes", &["tea"], &[])],
      Some(ROOM),
      OTHER_AGENT,
    )
    .await
    .unwrap();

  let companion = table
    .rows(&FragmentFilter {
      agent: Some(OTHER_AGENT.into()),
      ..of_kind("library-knowledge")
    })
    .await
    .unwrap();

  assert_eq!((companion.len(), companion[0].chat_id, companion[0].position), (1, Some(ROOM), 0));
  assert!(dir.path().join(COMPANION).join("knowledge/index.md").exists());

  fs::remove_file(dir.path().join("knowledge/restart.md")).unwrap();

  let done = table.reconcile(Some(AGENT)).await.unwrap();
  let knowledge = done.iter().find(|one| one.kind == "library-knowledge").unwrap();

  assert_eq!((knowledge.sync.retired, knowledge.entries, knowledge.trouble.clone()), (1, 0, None));
  assert_eq!(table.draw_indexes(Some(AGENT)).await.unwrap(), vec!["knowledge/index.md".to_owned()]);
  assert_eq!(
    fs::read_to_string(dir.path().join("knowledge/index.md")).unwrap(),
    "# Knowledge index\n"
  );

  let standing = table
    .rows(&FragmentFilter {
      active: Some(true),
      ..of_kind("library-knowledge")
    })
    .await
    .unwrap();

  assert_eq!(standing.iter().map(|row| row.agent.as_str()).collect::<Vec<_>>(), vec![OTHER_AGENT]);
}

#[tokio::test]
async fn a_brain_that_is_no_repository_is_passed_over() {
  let scratch = Scratch::new().await;
  let agency = agency();
  let receiver = FakeReceiver::default();
  let dir = tempfile::tempdir().unwrap();
  let table = Table::of(scratch.db(), &agency, &receiver, dir.path(), AGENT).await.unwrap();
  let done = table.reconcile(None).await.unwrap();

  assert_eq!(done.len(), 3);
  assert!(done.iter().all(|one| one.trouble.is_some()));
  assert!(table.draw_indexes(None).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_writer_is_handed_the_named_brain_pieces_on_disk_alone() {
  let scratch = Scratch::new().await;
  let agency = agency();
  let receiver = FakeReceiver::default();
  let dir = laid_brain(&[("rooms/-5/people/chunks/p9-tools.md", "# Tools\n")]);
  let table = Table::of(scratch.db(), &agency, &receiver, dir.path(), AGENT).await.unwrap();

  receiver.names.lock().unwrap().insert(9, "Tester".into());

  let rows = vec![
    Row {
      path: "rooms/-5/people/chunks/p9-tools.md".into(),
      keywords: vec!["herdr".into()],
      ..row(1, "person-book", AGENT, Some(-5), Some(9), "p9-tools", 0)
    },
    Row {
      path: "rooms/-5/people/chunks/p9-gone.md".into(),
      keywords: vec!["herdr".into()],
      ..row(2, "person-book", AGENT, Some(-5), Some(9), "p9-gone", 1)
    },
    Row {
      path: "rooms/-5/people/chunks/p9-tools.md".into(),
      keywords: vec!["herdr".into()],
      ..row(3, "lore", AGENT, None, None, "tools", 0)
    },
  ];
  let handed = table
    .pieces_for_writer(
      &rows,
      &BatchText {
        text: "Tester asked about herdr",
        speakers: &[9],
      },
      |_| false,
    )
    .await
    .unwrap();

  assert_eq!(handed.len(), 1);
  assert_eq!(
    (handed[0].path.as_str(), handed[0].text.as_str()),
    ("rooms/-5/people/chunks/p9-tools.md", "# Tools\n")
  );
}

#[tokio::test]
async fn a_shipped_library_is_brought_to_its_owners_entries_with_the_hand_honoured() {
  let scratch = Scratch::new().await;
  let agency = agency();
  let receiver = FakeReceiver::default();
  let table = Table::of(scratch.db(), &agency, &receiver, Path::new("/brain"), AGENT).await.unwrap();

  assert_eq!(
    table.reconcile_shipped("person-book", "Person book", &[entry("a", "A", 0)]).await,
    Err(ShippedFailure::BrainLibrary)
  );

  let first = table
    .reconcile_shipped("lore", "Lore", &[entry("a", "A", 0), entry("b", "B", 1)])
    .await
    .unwrap();

  assert_eq!(
    (first.entries, first.sync, first.reread),
    (
      2,
      Sync {
        added: 2,
        ..Sync::default()
      },
      2
    )
  );

  let rows = table.rows(&of_kind("lore")).await.unwrap();

  assert!(rows.iter().all(|row| row.agent == AGENT && row.active && row.status == "open"));
  assert_eq!(
    (rows[0].written_on.as_deref(), rows[0].body_hash.as_deref(), rows[0].path.as_str()),
    (Some("2026-01-02"), Some("hash-a"), "docs/a.md")
  );

  scratch
    .run(r#"UPDATE "fragments" SET "active" = 0, "hand" = 0 WHERE "slug" = 'b'"#, vec![])
    .await;

  let second = table
    .reconcile_shipped("lore", "Lore", &[entry("a", "A renamed", 0), entry("b", "B", 1)])
    .await
    .unwrap();

  assert_eq!(
    (second.sync, second.reread),
    (
      Sync {
        updated: 1,
        ..Sync::default()
      },
      0
    )
  );
  assert!(
    !table
      .rows(&of_kind("lore"))
      .await
      .unwrap()
      .iter()
      .find(|row| row.slug == "b")
      .unwrap()
      .active
  );

  let emptied = table.reconcile_shipped("lore", "Lore", &[]).await.unwrap();

  assert_eq!(
    emptied.sync,
    Sync {
      retired: 1,
      ..Sync::default()
    }
  );

  let rows = table.rows(&of_kind("lore")).await.unwrap();

  assert!(rows.iter().all(|row| !row.active && row.hand.is_none()));
  assert_eq!(rows.iter().find(|row| row.slug == "a").unwrap().title, "A renamed");
}

#[tokio::test]
async fn a_row_a_person_took_down_stays_down_when_its_piece_is_written_again() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let new_row = store::NewRow {
    kind: "library-knowledge".into(),
    agent: AGENT.into(),
    slug: "a".into(),
    title: "A".into(),
    path: "knowledge/a.md".into(),
    ..store::NewRow::default()
  };
  let made = store::write_row(db, &new_row, 1).await.unwrap();

  scratch
    .run(
      r#"UPDATE "fragments" SET "active" = 0, "hand" = 0 WHERE "id" = $1"#,
      vec![made.id.into()],
    )
    .await;

  let again = store::write_row(
    db,
    &store::NewRow {
      title: "A again".into(),
      ..new_row.clone()
    },
    2,
  )
  .await
  .unwrap();

  assert_eq!(
    (again.id, again.active, again.hand, again.title.as_str()),
    (made.id, false, Some(false), "A again")
  );

  store::retire_row(db, made.id).await.unwrap();

  let after = store::rows(db, &of_kind("library-knowledge")).await.unwrap();

  assert_eq!((after.len(), after[0].active, after[0].hand), (1, false, None));
}
