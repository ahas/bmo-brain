//! What the brain's screen reads and writes beyond the routes every caller uses, through the served routes
//! (docs/boundary-api.md, 「Memories」, 「Subjects」 and 「The console bundle」): the list's further filters and
//! orders, the tallies, a move's reason and its closing words, a restoration, a subject's name, and how far
//! the console's token reaches.

use crate::support;

use bmo_brain_client::{
  BrainClient, MemoriesQuery, MemoryMove, MemoryStatus, MemoryVersion, NewBrain, NewLayer, NewMemory,
  Restoration, SubjectKey, SubjectsQuery, TalliesQuery, TallyBy,
};
use support::{CONSOLE_TOKEN, Served, TOKEN, labels, refused};

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

fn moved(status: MemoryStatus) -> MemoryMove {
  MemoryMove {
    status,
    at: None,
    actor_name: "writer-b".to_owned(),
    reason: None,
    text: None,
  }
}

fn tallies(by: TallyBy) -> TalliesQuery {
  TalliesQuery {
    by,
    kind: None,
    status: None,
    exclude_types: None,
    layer_id: None,
    unfiled: None,
    subject_id: None,
    key: None,
    lineage_id: None,
    ids: None,
    superseded_by: None,
    label: None,
    q: None,
    situation: None,
  }
}

async fn brain_of(client: &BrainClient) -> i64 {
  client
    .brain_add(&NewBrain {
      account_key: "account-a".to_owned(),
      agent_key: "agent-a".to_owned(),
      name: None,
    })
    .await
    .unwrap()
    .id
}

fn ids(page: &bmo_brain_client::Page<bmo_brain_client::Memory>) -> Vec<i64> {
  page.items.iter().map(|memory| memory.id).collect()
}

#[tokio::test]
async fn the_list_takes_several_statuses_and_types_leaves_types_out_and_names_rows() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let fact = client.memory_add(brain, &memory("fact", "A fact")).await.unwrap();
  let pending = client
    .memory_add(
      brain,
      &NewMemory {
        status: Some(MemoryStatus::Pending),
        ..memory("situation", "A wait")
      },
    )
    .await
    .unwrap();
  let episode = client.memory_add(brain, &memory("episode", "Said once")).await.unwrap();
  let done = client.memory_add(brain, &memory("fact", "Finished")).await.unwrap();
  client.memory_move(brain, done.id, &moved(MemoryStatus::Done)).await.unwrap();
  let asked = |query: MemoriesQuery| async move { ids(&client.memories(brain, &query).await.unwrap()) };

  assert_eq!(
    asked(MemoriesQuery {
      status: Some("active,pending".to_owned()),
      ..MemoriesQuery::default()
    })
    .await,
    [fact.id, pending.id, episode.id]
  );
  assert_eq!(
    asked(MemoriesQuery {
      status: Some("active,pending".to_owned()),
      exclude_types: Some("episode".to_owned()),
      ..MemoriesQuery::default()
    })
    .await,
    [fact.id, pending.id]
  );
  assert_eq!(
    asked(MemoriesQuery {
      kind: Some("situation,episode".to_owned()),
      ..MemoriesQuery::default()
    })
    .await,
    [pending.id, episode.id]
  );
  assert_eq!(
    asked(MemoriesQuery {
      ids: Some(format!("{},{}", done.id, fact.id)),
      ..MemoriesQuery::default()
    })
    .await,
    [fact.id, done.id]
  );
  let bad = client
    .memories(
      brain,
      &MemoriesQuery {
        status: Some("active,asleep".to_owned()),
        ..MemoriesQuery::default()
      },
    )
    .await;
  assert_eq!(refused(bad), (400, "bad-request".to_owned()));
  let bad = client
    .memories(
      brain,
      &MemoriesQuery {
        ids: Some("1,two".to_owned()),
        ..MemoriesQuery::default()
      },
    )
    .await;
  assert_eq!(refused(bad), (400, "bad-request".to_owned()));
}

#[tokio::test]
async fn the_list_finds_the_unfiled_and_the_rows_a_version_superseded() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let layer = client
    .layer_add(
      brain,
      &NewLayer {
        name: "base".to_owned(),
        description: String::new(),
        position: None,
      },
    )
    .await
    .unwrap();
  let filed = client
    .memory_add(
      brain,
      &NewMemory {
        layer_id: Some(layer.id),
        ..memory("persona", "A piece")
      },
    )
    .await
    .unwrap();
  let unfiled = client.memory_add(brain, &memory("fact", "Loose")).await.unwrap();
  let version = client
    .memory_version(
      brain,
      unfiled.id,
      &MemoryVersion {
        text: Some("Loose, and fuller".to_owned()),
        kind: None,
        layer_id: None,
        position: None,
        subject_ids: None,
        subjects: None,
        importance: None,
        valid_from: None,
        recall_condition: None,
        release_condition: None,
        actor_name: "writer-b".to_owned(),
        labels: None,
      },
    )
    .await
    .unwrap();

  let listed = client
    .memories(
      brain,
      &MemoriesQuery {
        unfiled: Some(true),
        ..MemoriesQuery::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(ids(&listed), [unfiled.id, version.id]);
  assert!(!ids(&listed).contains(&filed.id));
  let superseded = client
    .memories(
      brain,
      &MemoriesQuery {
        superseded_by: Some(version.id.to_string()),
        ..MemoriesQuery::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(ids(&superseded), [unfiled.id]);
}

#[tokio::test]
async fn the_list_is_ordered_by_the_field_named_and_paged_in_that_order() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let mut written = Vec::new();
  // Importance and validFrom run against the ids, so an order by either is not the order of writing.
  for (index, importance) in [3, 9, 5, 9, 1].into_iter().enumerate() {
    let row = client
      .memory_add(
        brain,
        &NewMemory {
          importance: Some(importance),
          valid_from: Some(10_000 - index as i64 * 100),
          position: Some(4 - index as i64),
          ..memory("fact", &format!("Row {index}"))
        },
      )
      .await
      .unwrap();
    written.push(row.id);
  }
  let walked = |order: &'static str| async move {
    let mut seen = Vec::new();
    let mut after = None;
    loop {
      let page = client
        .memories(
          brain,
          &MemoriesQuery {
            order: Some(order.to_owned()),
            after,
            limit: Some(2),
            ..MemoriesQuery::default()
          },
        )
        .await
        .unwrap();
      seen.extend(ids(&page));
      after = page.items.last().map(|memory| memory.id);
      if !page.more {
        return seen;
      }
    }
  };

  let w = &written;
  assert_eq!(walked("validFrom").await, [w[4], w[3], w[2], w[1], w[0]]);
  assert_eq!(walked("-validFrom").await, *w);
  assert_eq!(walked("position").await, [w[4], w[3], w[2], w[1], w[0]]);
  assert_eq!(walked("-importance").await, [w[3], w[1], w[2], w[0], w[4]]);
  assert_eq!(walked("importance").await, [w[4], w[0], w[2], w[1], w[3]]);
  assert_eq!(walked("-id").await, [w[4], w[3], w[2], w[1], w[0]]);
  let bad = client
    .memories(
      brain,
      &MemoriesQuery {
        order: Some("text".to_owned()),
        ..MemoriesQuery::default()
      },
    )
    .await;
  assert_eq!(refused(bad), (400, "bad-request".to_owned()));
}

#[tokio::test]
async fn a_tally_counts_the_rows_of_each_group_under_the_lists_filters() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let layer = client
    .layer_add(
      brain,
      &NewLayer {
        name: "lore".to_owned(),
        description: String::new(),
        position: None,
      },
    )
    .await
    .unwrap();
  let person = SubjectKey {
    kind: "person".to_owned(),
    key: "writer-a".to_owned(),
  };
  client
    .memory_add(
      brain,
      &NewMemory {
        importance: Some(8),
        subjects: vec![person.clone()],
        ..memory("fact", "One")
      },
    )
    .await
    .unwrap();
  client
    .memory_add(
      brain,
      &NewMemory {
        importance: Some(4),
        layer_id: Some(layer.id),
        subjects: vec![person.clone()],
        ..memory("fact", "Two")
      },
    )
    .await
    .unwrap();
  let closed = client
    .memory_add(
      brain,
      &NewMemory {
        importance: Some(2),
        layer_id: Some(layer.id),
        ..memory("lore", "Three")
      },
    )
    .await
    .unwrap();
  client.memory_move(brain, closed.id, &moved(MemoryStatus::Invalidated)).await.unwrap();

  let by_type = client.tallies(brain, &tallies(TallyBy::Type)).await.unwrap();
  let groups: Vec<(Option<&str>, i64, f64)> = by_type
    .iter()
    .map(|tally| (tally.group.as_deref(), tally.count, tally.importance))
    .collect();
  assert_eq!(groups, [(Some("fact"), 2, 6.0), (Some("lore"), 1, 2.0)]);
  let live = client
    .tallies(
      brain,
      &TalliesQuery {
        status: Some("active,pending".to_owned()),
        ..tallies(TallyBy::Layer)
      },
    )
    .await
    .unwrap();
  let layer_key = layer.id.to_string();
  let groups: Vec<(Option<&str>, i64)> =
    live.iter().map(|tally| (tally.group.as_deref(), tally.count)).collect();
  assert_eq!(groups, [(None, 1), (Some(layer_key.as_str()), 1)]);
  let by_status = client.tallies(brain, &tallies(TallyBy::Status)).await.unwrap();
  let groups: Vec<(Option<&str>, i64)> =
    by_status.iter().map(|tally| (tally.group.as_deref(), tally.count)).collect();
  assert_eq!(groups, [(Some("active"), 2), (Some("invalidated"), 1)]);
  let by_subject = client.tallies(brain, &tallies(TallyBy::Subject)).await.unwrap();
  assert_eq!(by_subject[0].count, 2);
  assert!(by_subject[0].group.is_some());
  assert_eq!((by_subject[1].group.clone(), by_subject[1].count), (None, 1));
  assert!(by_subject.iter().all(|tally| tally.strength == 1.0 && tally.retrieval_strength == 0.0));
}

#[tokio::test]
async fn a_move_keeps_its_reason_and_a_move_to_done_takes_the_words_it_ended_on() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let wrong = client.memory_add(brain, &memory("fact", "Wrong")).await.unwrap();
  let invalidated = client
    .memory_move(
      brain,
      wrong.id,
      &MemoryMove {
        reason: Some("It never held.".to_owned()),
        ..moved(MemoryStatus::Invalidated)
      },
    )
    .await
    .unwrap();
  assert_eq!(invalidated.reason.as_deref(), Some("It never held."));
  assert_eq!(client.memory(brain, wrong.id).await.unwrap().reason.as_deref(), Some("It never held."));

  let errand = client.memory_add(brain, &memory("situation", "Bring the parcel")).await.unwrap();
  let ended = client
    .memory_move(
      brain,
      errand.id,
      &MemoryMove {
        text: Some("Brought the parcel on the second day".to_owned()),
        ..moved(MemoryStatus::Done)
      },
    )
    .await
    .unwrap();
  assert_ne!(ended.id, errand.id);
  assert_eq!((ended.status, ended.lineage_id), (MemoryStatus::Done, errand.id));
  assert_eq!(ended.text, "Brought the parcel on the second day");
  let before = client.memory(brain, errand.id).await.unwrap();
  assert_eq!((before.status, before.superseded_by), (MemoryStatus::Superseded, Some(ended.id)));

  let other = client.memory_add(brain, &memory("fact", "Other")).await.unwrap();
  let refused_text = client
    .memory_move(
      brain,
      other.id,
      &MemoryMove {
        text: Some("Words".to_owned()),
        ..moved(MemoryStatus::Invalidated)
      },
    )
    .await;
  assert_eq!(refused(refused_text), (400, "bad-request".to_owned()));
  let closed_again = client
    .memory_move(
      brain,
      ended.id,
      &MemoryMove {
        text: Some("Again".to_owned()),
        ..moved(MemoryStatus::Done)
      },
    )
    .await;
  assert_eq!(refused(closed_again), (409, "conflict".to_owned()));
  assert_eq!(client.lineage(brain, errand.id).await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_closed_lineage_is_restored_by_a_new_version_and_only_from_its_newest_row() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let keyed = client
    .memory_add(
      brain,
      &NewMemory {
        key: Some("piece-a".to_owned()),
        ..memory("knowledge", "A piece")
      },
    )
    .await
    .unwrap();
  let restore = |id: i64, text: Option<&str>| {
    let body = Restoration {
      text: text.map(str::to_owned),
      actor_name: "screen".to_owned(),
    };
    async move { client.memory_restore(brain, id, &body).await }
  };
  assert_eq!(refused(restore(keyed.id, None).await), (409, "conflict".to_owned()));
  client.memory_move(brain, keyed.id, &moved(MemoryStatus::Invalidated)).await.unwrap();
  let restored = restore(keyed.id, None).await.unwrap();
  assert_eq!(
    (restored.status, restored.lineage_id, restored.text.as_str(), restored.key.as_deref()),
    (MemoryStatus::Active, keyed.id, "A piece", Some("piece-a"))
  );
  let closed = client.memory(brain, keyed.id).await.unwrap();
  assert_eq!((closed.status, closed.superseded_by), (MemoryStatus::Invalidated, None));
  assert_eq!(refused(restore(keyed.id, None).await), (409, "conflict".to_owned()));

  client.memory_move(brain, restored.id, &moved(MemoryStatus::Done)).await.unwrap();
  let taken = client
    .memory_add(
      brain,
      &NewMemory {
        key: Some("piece-a".to_owned()),
        ..memory("knowledge", "Another holds the key")
      },
    )
    .await
    .unwrap();
  assert_eq!(refused(restore(restored.id, Some("Back again")).await), (409, "conflict".to_owned()));
  client.memory_move(brain, taken.id, &moved(MemoryStatus::Invalidated)).await.unwrap();
  let again = restore(restored.id, Some("Back again")).await.unwrap();
  assert_eq!((again.text.as_str(), again.lineage_id), ("Back again", keyed.id));
  assert_eq!(client.lineage(brain, keyed.id).await.unwrap().len(), 3);
  assert_eq!(refused(restore(999_999, None).await), (404, "not-found".to_owned()));
}

#[tokio::test]
async fn a_subject_is_called_by_its_newest_active_name_and_listed_by_ids() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let person = SubjectKey {
    kind: "person".to_owned(),
    key: "writer-a".to_owned(),
  };
  let room = SubjectKey {
    kind: "room".to_owned(),
    key: "room:a".to_owned(),
  };
  client
    .memory_add(
      brain,
      &NewMemory {
        subjects: vec![person.clone()],
        valid_from: Some(1_000),
        ..memory("name", "Writer A")
      },
    )
    .await
    .unwrap();
  client
    .memory_add(
      brain,
      &NewMemory {
        subjects: vec![person.clone(), room.clone()],
        valid_from: Some(2_000),
        ..memory("fact", "Writes in room a")
      },
    )
    .await
    .unwrap();
  let subjects = client.subjects(brain, &SubjectsQuery::default()).await.unwrap().items;
  let (person_id, room_id) = (subjects[0].id, subjects[1].id);
  assert_eq!(subjects[0].name.as_deref(), Some("Writer A"));
  assert_eq!(subjects[1].name, None);
  let newer = client
    .memory_add(
      brain,
      &NewMemory {
        subjects: vec![person.clone()],
        valid_from: Some(3_000),
        ..memory("name", "The writer")
      },
    )
    .await
    .unwrap();
  assert_eq!(client.subject(brain, person_id).await.unwrap().name.as_deref(), Some("The writer"));
  client.memory_move(brain, newer.id, &moved(MemoryStatus::Invalidated)).await.unwrap();
  assert_eq!(client.subject(brain, person_id).await.unwrap().name.as_deref(), Some("Writer A"));

  let named = client
    .subjects(
      brain,
      &SubjectsQuery {
        ids: Some(room_id.to_string()),
        ..SubjectsQuery::default()
      },
    )
    .await
    .unwrap();
  assert_eq!(named.items.iter().map(|subject| subject.id).collect::<Vec<_>>(), [room_id]);
}

/// A call as the console relays it: the console's token, and the role where one is given.
async fn relayed(
  served: &Served,
  method: reqwest::Method,
  path: &str,
  role: Option<&str>,
) -> (u16, String) {
  let mut request = reqwest::Client::new()
    .request(method, format!("{}{path}", served.base))
    .bearer_auth(CONSOLE_TOKEN)
    .header("Accept", "application/json");
  if let Some(role) = role {
    request = request.header("Owner-Role", role);
  }
  let answer = request.send().await.unwrap();
  let status = answer.status().as_u16();
  (status, answer.text().await.unwrap())
}

fn code(body: &str) -> String {
  serde_json::from_str::<serde_json::Value>(body).unwrap()["code"]
    .as_str()
    .unwrap_or_default()
    .to_owned()
}

#[tokio::test]
async fn the_consoles_token_reaches_the_screens_routes_with_the_administrators_role_and_nothing_else() {
  let served = Served::new().await;
  let brain = brain_of(&served.client).await;
  let get = reqwest::Method::GET;

  let (status, body) = relayed(&served, get.clone(), "/brains", Some("administrator")).await;
  assert_eq!(status, 200, "{body}");
  let (status, body) = relayed(&served, get.clone(), "/brains", None).await;
  assert_eq!((status, code(&body)), (403, "forbidden".to_owned()));
  let (status, body) = relayed(&served, get.clone(), "/brains", Some("member")).await;
  assert_eq!((status, code(&body)), (403, "forbidden".to_owned()));
  let path = format!("/brains/{brain}/tallies?by=type");
  assert_eq!(relayed(&served, get.clone(), &path, Some("administrator")).await.0, 200);
  let path = format!("/brains/{brain}/memories?status=active,pending&order=-validFrom");
  assert_eq!(relayed(&served, get.clone(), &path, Some("administrator")).await.0, 200);
  let path = format!("/brains/{brain}/unconsolidated");
  let (status, body) = relayed(&served, get.clone(), &path, Some("administrator")).await;
  assert_eq!((status, code(&body)), (403, "forbidden".to_owned()));
  let (status, body) =
    relayed(&served, reqwest::Method::DELETE, &format!("/brains/{brain}"), Some("administrator")).await;
  assert_eq!((status, code(&body)), (403, "forbidden".to_owned()));
  let (status, body) = relayed(&served, get.clone(), "/discipline", Some("administrator")).await;
  assert_eq!((status, code(&body)), (403, "forbidden".to_owned()));
  // With no bundle built, the manifest is not found, so a console leaves the brain out.
  let (status, body) = relayed(&served, get.clone(), "/console-manifest", None).await;
  assert_eq!((status, code(&body)), (404, "not-found".to_owned()));
  // The service's token reaches every route as before, with no role.
  assert!(served.client.unconsolidated(brain, &Default::default()).await.is_ok());
  let stranger = served.with_token("another-token").brains(&Default::default()).await;
  assert_eq!(refused(stranger), (401, "unauthorized".to_owned()));
  assert_ne!(TOKEN, CONSOLE_TOKEN);
}

#[tokio::test]
async fn the_manifest_names_the_built_module_stylesheet_and_words_and_each_is_answered() {
  let folder = std::env::temp_dir().join(format!("brain-console-{}", std::process::id()));
  std::fs::create_dir_all(&folder).unwrap();
  std::fs::write(folder.join("module.js"), "export default { routes: [] };\n").unwrap();
  std::fs::write(folder.join("style.css"), ".brain {}\n").unwrap();
  std::fs::write(folder.join("words.json"), r#"{"ko":{"brain":{"title":{"screen":"x"}}}}"#).unwrap();
  let served = Served::with_bundle(&folder).await;
  std::fs::remove_dir_all(&folder).unwrap();
  let get = reqwest::Method::GET;

  let (status, body) = relayed(&served, get.clone(), "/console-manifest", None).await;
  assert_eq!(status, 200, "{body}");
  let manifest: bmo_brain_client::ConsoleManifest = serde_json::from_str(&body).unwrap();
  assert_eq!(
    (manifest.name.as_str(), manifest.screen_path.as_str(), manifest.service_name.as_str()),
    ("brain", "/brain", "brain")
  );
  assert_eq!(
    (manifest.role, manifest.owner_key, manifest.events),
    (bmo_brain_client::Role::Administrator, false, true)
  );
  assert!(manifest.bundle.starts_with("/console-bundle/module.") && manifest.bundle.ends_with(".js"));
  let stylesheet = manifest.stylesheet.clone().unwrap();
  let read = read_by_contract(&body);
  assert_eq!(read["role"], "administrator", "{read}");
  assert_eq!(read["apiPrefix"], "/");
  assert_eq!(read["calls"], serde_json::json!([]));
  assert_eq!(served.client.console_manifest().await.unwrap(), manifest);

  let (status, body) = relayed(&served, get.clone(), &manifest.words, None).await;
  assert_eq!((status, body.as_str()), (200, r#"{"ko":{"brain":{"title":{"screen":"x"}}}}"#));
  let (status, _) = relayed(&served, get.clone(), &manifest.bundle, None).await;
  assert_eq!(status, 403);
  let (status, body) = relayed(&served, get.clone(), &manifest.bundle, Some("administrator")).await;
  assert_eq!((status, body.as_str()), (200, "export default { routes: [] };\n"));
  let (status, body) = relayed(&served, get.clone(), &stylesheet, Some("administrator")).await;
  assert_eq!((status, body.as_str()), (200, ".brain {}\n"));
  let (status, body) =
    relayed(&served, get, "/console-bundle/module.0000000000000000.js", Some("administrator")).await;
  assert_eq!((status, code(&body)), (404, "not-found".to_owned()));
}

#[tokio::test]
async fn the_manifest_names_the_services_the_settings_say_the_screen_calls() {
  let folder = std::env::temp_dir().join(format!("brain-console-calls-{}", std::process::id()));
  std::fs::create_dir_all(&folder).unwrap();
  std::fs::write(folder.join("module.js"), "export default { routes: [] };\n").unwrap();
  std::fs::write(folder.join("words.json"), r#"{"ko":{"brain":{"title":{"screen":"x"}}}}"#).unwrap();
  let served = Served::with_bundle_calling(&folder, &["words-out"]).await;
  std::fs::remove_dir_all(&folder).unwrap();

  let (status, body) = relayed(&served, reqwest::Method::GET, "/console-manifest", None).await;
  assert_eq!(status, 200, "{body}");
  let manifest: bmo_brain_client::ConsoleManifest = serde_json::from_str(&body).unwrap();
  assert_eq!(manifest.calls, vec!["words-out".to_owned()]);
  assert_eq!(read_by_contract(&body)["calls"], serde_json::json!(["words-out"]));
}

/// The manifest read through bmo-ui's own reading of the contract, by the client's script under `bun`.
fn read_by_contract(manifest: &str) -> serde_json::Value {
  use std::io::Write as _;
  use std::process::{Command, Stdio};

  let mut reading = Command::new("bun")
    .args(["tests/read-manifest.ts"])
    .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/client"))
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("bun runs in client/");
  reading
    .stdin
    .take()
    .expect("its input")
    .write_all(manifest.as_bytes())
    .expect("the manifest is handed over");
  let read = reading.wait_with_output().expect("the reading ends");
  assert!(
    read.status.success(),
    "the contract refuses the manifest: {}",
    String::from_utf8_lossy(&read.stderr)
  );
  serde_json::from_slice(&read.stdout).expect("the reading answers JSON")
}
