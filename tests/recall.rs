//! Recall by relevance (docs/design.md, 「Recall」): a turn's words reach a row by their stems and never by a
//! stop word, a cue few rows share spreads more than one many share, the order goes by relevance, then
//! importance, then the latest use, and Maximal Marginal Relevance puts a near-double behind a row unlike the
//! ones in.

use crate::support;

use bmo_brain::store::words::{overlap, stems};
use bmo_brain_client::{
  BrainClient, Hits, NewBrain, NewMemory, Recall, RecallAsk, RecallPart, Rules, SimilarAsk, SubjectKey,
};
use support::{Served, labels};

fn subject(kind: &str, key: &str) -> SubjectKey {
  SubjectKey {
    kind: kind.to_owned(),
    key: key.to_owned(),
  }
}

fn memory(text: &str, importance: i64, subjects: &[SubjectKey]) -> NewMemory {
  NewMemory {
    key: None,
    kind: "note".to_owned(),
    text: text.to_owned(),
    layer_id: None,
    position: None,
    subject_ids: vec![],
    subjects: subjects.to_vec(),
    importance: Some(importance),
    valid_from: None,
    recall_condition: None,
    release_condition: None,
    status: None,
    actor_name: "writer-a".to_owned(),
    labels: labels(&[]),
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

fn ask(speakers: &[SubjectKey], words: &str, budget: Option<i64>) -> RecallAsk {
  RecallAsk {
    room: None,
    speakers: speakers.to_vec(),
    words: words.to_owned(),
    session_key: None,
    session_opened_at: None,
    budget_tokens: budget,
    record: Some(false),
  }
}

fn searched(recall: &Recall) -> Vec<i64> {
  recall
    .chosen
    .iter()
    .filter(|c| c.part == RecallPart::Searched)
    .map(|c| c.memory_id)
    .collect()
}

#[test]
fn a_texts_stems_leave_the_stop_words_out_and_meet_by_their_jaccard_overlap() {
  let said = stems("The kiln fires at dawn, and the member's kilns are firing.");
  assert_eq!(said.into_iter().collect::<Vec<_>>(), ["dawn", "fire", "kiln", "member"]);
  assert!(stems("what is it that was there").is_empty());
  assert_eq!(overlap(&stems("orchard ladder barn"), &stems("orchard ladder loft")), 2.0 / 4.0);
  assert_eq!(overlap(&stems(""), &stems("the")), 0.0);
}

#[tokio::test]
async fn a_word_reaches_a_row_by_its_stem_and_a_stop_word_reaches_none() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let kiln = client.memory_add(brain, &memory("The kiln fires at dawn.", 5, &[])).await.unwrap();
  let well = client.memory_add(brain, &memory("The well is deep.", 5, &[])).await.unwrap();
  let stemmed = client.recall(brain, &ask(&[], "firing kilns", None)).await.unwrap();
  assert_eq!(searched(&stemmed), [kiln.id], "{:?}", stemmed.chosen);
  let stopped = client.recall(brain, &ask(&[], "what is it that was there", None)).await.unwrap();
  assert!(searched(&stopped).is_empty(), "{:?}", stopped.chosen);
  // The likes of a candidate are found by the same stems.
  let likes = client
    .similar(
      brain,
      &SimilarAsk {
        text: "Is the kiln firing yet?".to_owned(),
        subjects: vec![],
        limit: None,
      },
    )
    .await
    .unwrap();
  assert_eq!(likes.iter().map(|m| m.id).collect::<Vec<_>>(), [kiln.id]);
  let _ = well;
}

#[tokio::test]
async fn a_subject_few_rows_share_outranks_one_many_share() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let common = subject("person", "person:common");
  let rare = subject("person", "person:rare");
  for n in 0..10 {
    client
      .memory_add(brain, &memory(&format!("Common note {n}."), 5, std::slice::from_ref(&common)))
      .await
      .unwrap();
  }
  let alone = client
    .memory_add(brain, &memory("The only rare note.", 5, std::slice::from_ref(&rare)))
    .await
    .unwrap();
  let recall = client.recall(brain, &ask(&[common.clone(), rare.clone()], "", None)).await.unwrap();
  let chosen = searched(&recall);
  assert_eq!(chosen.first(), Some(&alone.id), "{:?}", recall.chosen);
  let rules = Rules::default();
  // Two cues among eleven live rows: the rare subject's fan is one, the common one's ten.
  let alone_score = recall.chosen.iter().find(|c| c.memory_id == alone.id).unwrap().score;
  assert!((alone_score - 11_f64.ln() / 2.0).abs() < 1e-9, "{alone_score}");
  let common_score = recall.chosen.iter().find(|c| c.memory_id != alone.id).unwrap().score;
  let expected = (11.0_f64 / 10.0).ln() / 2.0;
  assert!((common_score - expected).abs() < 1e-9, "{common_score} against {expected}");
  // The common subject's rows stand too, no more than the cap of them.
  assert_eq!(chosen.len() - 1, rules.subject_cap as usize, "{:?}", recall.chosen);
}

#[tokio::test]
async fn a_row_matching_two_cues_outranks_one_matching_one_of_them_at_hundreds_of_rows_a_cue() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  // Six hundred live rows: three hundred about home, two hundred about the desktop, a hundred about neither.
  for (stem, count) in [("Home", 300), ("Desktop", 200), ("Other", 100)] {
    sqlx::query(
      "INSERT INTO memories (brain_id, type, text, status, lineage_id, valid_from, importance, actor_name, \
       labels, created_at) SELECT $1, 'note', $2 || ' note ' || n || '.', 'active', 0, 1, 5, 'writer-a', '{}', 1 \
       FROM generate_series(1, $3) AS n",
    )
    .bind(brain)
    .bind(stem)
    .bind(count)
    .execute(&served.store.pool)
    .await
    .unwrap();
  }
  let both = client
    .memory_add(brain, &memory("The home desktop sleeps at night.", 3, &[]))
    .await
    .unwrap();
  let one = client
    .memory_add(brain, &memory("Home is where the garden grows.", 10, &[]))
    .await
    .unwrap();
  let recall = client.recall(brain, &ask(&[], "home desktop", None)).await.unwrap();
  let chosen = searched(&recall);
  assert_eq!(chosen.first(), Some(&both.id), "{:?}", recall.chosen);
  // Each cue gives half the log of the live rows over its fan: 602 rows, home in 302, desktop in 201. The row
  // matching both stands above every row matching one, the best of which match the rarer desktop alone.
  let live = 602.0_f64;
  let home = (live / 302.0).ln() / 2.0;
  let desktop = (live / 201.0).ln() / 2.0;
  assert!((recall.chosen[0].score - (home + desktop)).abs() < 1e-9, "{:?}", recall.chosen[0]);
  assert!((recall.chosen[1].score - desktop).abs() < 1e-9, "{:?}", recall.chosen[1]);
  assert!(recall.chosen.iter().skip(1).all(|c| c.score < home + desktop && c.score >= 0.0));
  let _ = one;
}

#[tokio::test]
async fn equal_relevance_goes_by_importance_and_then_by_the_latest_use() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let low = client.memory_add(brain, &memory("Tomatoes grow by the gate.", 3, &[])).await.unwrap();
  let high = client
    .memory_add(brain, &memory("Tomatoes ripen near the shed.", 7, &[]))
    .await
    .unwrap();
  let also = client.memory_add(brain, &memory("Tomatoes sold on market.", 3, &[])).await.unwrap();
  // The less important rows used, the later of them last: importance still goes first, then the latest use.
  for (n, row) in [low.id, also.id].into_iter().enumerate() {
    client
      .hits_record(
        brain,
        &Hits {
          memory_ids: vec![row],
          session_key: None,
          actor_name: "agent-session".to_owned(),
          at: Some(also.created_at + 1_000 * (n as i64 + 1)),
        },
      )
      .await
      .unwrap();
  }
  let recall = client.recall(brain, &ask(&[], "tomatoes", None)).await.unwrap();
  assert_eq!(searched(&recall), [high.id, also.id, low.id], "{:?}", recall.chosen);
}

#[tokio::test]
async fn maximal_marginal_relevance_puts_a_near_double_behind_a_row_unlike_the_ones_in() {
  let served = Served::new().await;
  let client = &served.client;
  let brain = brain_of(client).await;
  let first = client
    .memory_add(brain, &memory("The orchard ladder is stored in the barn loft.", 5, &[]))
    .await
    .unwrap();
  let double = client
    .memory_add(brain, &memory("The orchard ladder is stored in the barn loft now.", 5, &[]))
    .await
    .unwrap();
  let unlike = client
    .memory_add(brain, &memory("Orchard ladder rungs need fixing before harvest.", 5, &[]))
    .await
    .unwrap();
  let words = "where is the orchard ladder";
  let whole = client.recall(brain, &ask(&[], words, None)).await.unwrap();
  // The three are equally relevant, so the order alone would put the double second.
  assert_eq!(searched(&whole), [first.id, unlike.id, double.id], "{:?}", whole.chosen);
  // A budget with room for two lines leaves the near-double out.
  let room: i64 = whole.chosen.iter().take(2).map(|c| c.token_count).sum();
  let tight = client.recall(brain, &ask(&[], words, Some(room))).await.unwrap();
  assert_eq!(searched(&tight), [first.id, unlike.id], "{:?}", tight.chosen);
}
