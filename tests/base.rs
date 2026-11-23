//! The base and its candidates (docs/design.md, 「The base」 and 「Raising a memory to the base」;
//! docs/boundary-api.md, 「Raising candidates」 and 「Declaring the chain's kinds」): a source brain sifted by its
//! subjects, each memory left judged by a stand-in classifier, the candidates written as pending rows of the base
//! once and only once, a dry run writing nothing, and the chain's kinds as the deployment declares them.

use crate::support;

use axum::Json;
use axum::routing::post;
use bmo_brain::base::{ClassifierConfig, Judgement, Run, raise, read_answer};
use bmo_brain::chain::{GLOBAL, ORGANISATION, declaration};
use bmo_brain::store::base::{BASE_KEY, PassOver};
use bmo_brain_client::{
  BrainKind, BrainsQuery, MemoriesQuery, MemoryStatus, NewBrain, NewMemory, SubjectKey,
};
use serde_json::{Value, json};
use support::Served;

fn subject(kind: &str, key: &str) -> SubjectKey {
  SubjectKey {
    kind: kind.to_owned(),
    key: key.to_owned(),
  }
}

fn tip(kind: &str, text: &str, subjects: Vec<SubjectKey>) -> NewMemory {
  NewMemory {
    kind: kind.to_owned(),
    text: text.to_owned(),
    subjects,
    actor_name: "writer-a".to_owned(),
    ..NewMemory::default()
  }
}

/// A classifier standing in for the gateway: yes where the memory's words hold "anyone", no otherwise.
async fn classifier(folder: &std::path::Path) -> (ClassifierConfig, tokio::task::JoinHandle<()>) {
  let app = axum::Router::new().route(
    "/v1/chat/completions",
    post(|Json(ask): Json<Value>| async move {
      let words = ask["messages"][1]["content"].as_str().unwrap_or("").to_owned();
      let said = if words.contains("anyone") { "Yes, it holds for anyone." } else { "No. It is bound." };
      Json(json!({ "choices": [{ "message": { "content": said } }] }))
    }),
  );
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());
  let serving = tokio::spawn(async move {
    axum::serve(listener, app).await.unwrap();
  });
  let key_file = folder.join("gateway.key");
  std::fs::write(&key_file, "the-gateway-key\n").unwrap();
  let config = ClassifierConfig {
    address,
    key_file,
    model: "model-a".to_owned(),
    max_tokens: 40,
    concurrency: 2,
  };
  (config, serving)
}

#[tokio::test]
async fn a_source_is_sifted_by_subject_judged_and_raised_as_pending_rows_of_the_base_once() {
  let served = Served::new().await;
  let client = &served.client;
  let source = client
    .brain_add(&NewBrain {
      account_key: "account-a".to_owned(),
      agent_key: "agent-a".to_owned(),
      ..NewBrain::default()
    })
    .await
    .unwrap()
    .id;
  let fits = client
    .memory_add(
      source,
      &NewMemory {
        recall_condition: Some("a formatter runs".to_owned()),
        ..tip(
          "tip",
          "Run the formatter before the linter; it fits anyone.",
          vec![subject("tool", "formatter")],
        )
      },
    )
    .await
    .unwrap();
  for memory in [
    tip(
      "check",
      "Look at the suite's log of repo-a; anyone may.",
      vec![subject("repository", "repo-a")],
    ),
    tip("tip", "Ask the member first; anyone may.", vec![subject("person", "1")]),
    tip("tip", "The room's own way; anyone may.", vec![subject("room", "7")]),
    tip("knowledge", "The own unit's port; anyone may.", vec![subject("tool", "Own-Console")]),
    tip("tip", "Restart the service on host-a.", vec![]),
    tip("fact", "The suite passed; anyone may read it.", vec![]),
  ] {
    client.memory_add(source, &memory).await.unwrap();
  }
  let folder = std::env::temp_dir().join(format!("bmo-brain-base-{}", std::process::id()));
  std::fs::create_dir_all(&folder).unwrap();
  let (config, gateway) = classifier(&folder).await;
  let run = |dry_run: bool| Run {
    from: source,
    pass_over: PassOver {
      types: vec!["repository".to_owned()],
      key_prefixes: vec!["own-".to_owned()],
    },
    limit: None,
    dry_run,
  };

  let dry = raise(&served.store, Some(&config), &run(true)).await.unwrap();
  let sifted = &dry.sifted;
  assert_eq!(
    (sifted.considered, sifted.person_or_room, sifted.by_type, sifted.by_key, sifted.raised),
    (6, 2, 1, 1, 0)
  );
  assert_eq!((dry.judged, dry.fits, dry.does_not_fit, dry.unjudged, dry.written), (2, 1, 1, 0, 0));
  assert_eq!(dry.candidates, vec![(fits.id, None)]);
  let bases = BrainsQuery {
    kind: Some(BrainKind::Org),
    account_key: Some(BASE_KEY.to_owned()),
    agent_key: Some(BASE_KEY.to_owned()),
    ..BrainsQuery::default()
  };
  assert!(client.brains(&bases).await.unwrap().items.is_empty(), "a dry run makes no base");

  let came = raise(&served.store, Some(&config), &run(false)).await.unwrap();
  assert_eq!(came.written, 1);
  let base = client.brains(&bases).await.unwrap().items[0].clone();
  assert_eq!((base.kind, base.name.as_str()), (BrainKind::Org, "base"));
  let candidates = client
    .memories(
      base.id,
      &MemoriesQuery {
        status: Some("pending".to_owned()),
        ..MemoriesQuery::default()
      },
    )
    .await
    .unwrap()
    .items;
  assert_eq!(candidates.len(), 1);
  let candidate = &candidates[0];
  assert_eq!(
    (candidate.status, candidate.kind.as_str(), candidate.text.as_str()),
    (MemoryStatus::Pending, "tip", fits.text.as_str())
  );
  assert_eq!(candidate.recall_condition.as_deref(), Some("a formatter runs"));
  assert_eq!((candidate.locked, candidate.internal_only, candidate.shared), (false, false, false));
  assert_eq!(candidate.labels["sourceBrainId"], source.to_string());
  assert_eq!(candidate.labels["sourceMemoryId"], fits.id.to_string());
  assert_eq!(candidate.labels["candidateModel"], "model-a");
  assert_eq!(candidate.labels["candidateReason"], "it holds for anyone.");
  assert_eq!(candidate.subjects.iter().map(|s| s.kind.as_str()).collect::<Vec<_>>(), vec!["tool"]);
  assert_eq!(client.memory(source, fits.id).await.unwrap().status, MemoryStatus::Active);

  // A memory already raised is passed over, whatever became of its candidate.
  let again = raise(&served.store, Some(&config), &run(false)).await.unwrap();
  assert_eq!((again.sifted.raised, again.written), (1, 0));

  // With no classifier nothing is judged.
  let unjudged = raise(&served.store, None, &run(true)).await.unwrap();
  assert_eq!((unjudged.judged, unjudged.unjudged), (0, 1));
  gateway.abort();
  let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn an_answer_is_read_by_its_first_word() {
  assert_eq!(read_answer("Yes. It holds anywhere."), Judgement::Fits("It holds anywhere.".to_owned()));
  assert_eq!(read_answer("  no, bound to one host"), Judgement::DoesNotFit);
  assert!(matches!(read_answer("Perhaps."), Judgement::Unjudged(_)));
}

#[test]
fn the_chain_kinds_are_narrowing_flags_read_by_default_at_the_platform_and_units() {
  for key in [GLOBAL, ORGANISATION] {
    let declared = declaration(key, "What it says.");
    assert_eq!(declared["key"], key);
    assert_eq!(declared["declaredBy"], "brain");
    assert_eq!(
      (declared["shape"].as_str(), declared["mode"].as_str()),
      (Some("flag"), Some("narrowing"))
    );
    assert_eq!(declared["default"], json!(true));
    assert_eq!(declared["levels"], json!(["platform", "unit"]));
    assert_eq!(declared["unitPermission"], "manage");
  }
}
