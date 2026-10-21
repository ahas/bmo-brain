//! The caller's own records (docs/boundary-api.md, 「A caller's records」): a room's rules, a session's loaded
//! documents and a chat session's handoff note, kept against the real service and read back in a round.

mod support;

use bmo_brain_client::{
  AgentAsk, Load, LoadsKeep, NoteKeep, RoomRule, RoomRulesAsk, RuleSet, RuleText, SessionAsk,
};
use support::{Served, refused};

fn rule(id: &str, subject: i64, text: &str) -> RoomRule {
  RoomRule {
    id: id.into(),
    subject,
    text: text.into(),
  }
}

fn set(chat_id: i64, message_id: i64, id: &str, rule: Option<RuleText>) -> RuleSet {
  RuleSet {
    chat_id,
    message_id,
    id: id.into(),
    rule,
  }
}

fn words(subject: i64, text: &str) -> Option<RuleText> {
  Some(RuleText {
    subject,
    text: text.into(),
  })
}

#[tokio::test]
async fn a_rooms_rules_are_set_changed_revoked_and_read_in_order() {
  let served = Served::new().await;
  let client = &served.client;

  let first = client.room_rule_set(&set(-7, 11, "r1", words(42, "no spoilers"))).await.unwrap();
  assert_eq!(first.rules, [rule("r1", 42, "no spoilers")]);
  client.room_rule_set(&set(-7, 12, "r2", words(8, "be brief"))).await.unwrap();
  assert_eq!(
    client.room_rules(&RoomRulesAsk { chat_id: -7 }).await.unwrap().rules,
    [rule("r1", 42, "no spoilers"), rule("r2", 8, "be brief")]
  );

  // A rule set again takes the words and the subject given.
  let changed = client.room_rule_set(&set(-7, 13, "r1", words(43, "no hints"))).await.unwrap();
  assert_eq!(changed.rules, [rule("r1", 43, "no hints"), rule("r2", 8, "be brief")]);

  // A revoked rule leaves nothing behind.
  let revoked = client.room_rule_set(&set(-7, 14, "r1", None)).await.unwrap();
  assert_eq!(revoked.rules, [rule("r2", 8, "be brief")]);
  assert!(client.room_rules(&RoomRulesAsk { chat_id: -8 }).await.unwrap().rules.is_empty());

  assert_eq!(refused(client.room_rule_set(&set(-7, 15, "r3", words(1, "  "))).await).0, 400);
  assert_eq!(refused(client.room_rule_set(&set(-7, 15, "a b", words(1, "x"))).await).0, 400);
}

#[tokio::test]
async fn a_sessions_loads_are_kept_read_and_forgotten() {
  let served = Served::new().await;
  let client = &served.client;
  let a = Load {
    path: "docs/a.md".into(),
    version: "v1".into(),
  };
  let b = Load {
    path: "docs/b.md".into(),
    version: "v2".into(),
  };
  let ask = || SessionAsk {
    session: "s-1".into(),
  };

  client
    .session_loads_keep(&LoadsKeep {
      session: "s-1".into(),
      loads: vec![a.clone(), b.clone()],
      at: Some(1_000),
    })
    .await
    .unwrap();
  assert_eq!(client.session_loads(&ask()).await.unwrap().loads, [a.clone(), b.clone()]);

  // A path kept again takes the version it was read at.
  let again = Load {
    path: "docs/a.md".into(),
    version: "v3".into(),
  };
  client
    .session_loads_keep(&LoadsKeep {
      session: "s-1".into(),
      loads: vec![again.clone()],
      at: Some(1_100),
    })
    .await
    .unwrap();
  assert_eq!(client.session_loads(&ask()).await.unwrap().loads, [again, b]);

  // Another session's loads stand apart, and a fold lets them go.
  assert!(
    client
      .session_loads(&SessionAsk {
        session: "s-2".into()
      })
      .await
      .unwrap()
      .loads
      .is_empty()
  );
  client.session_loads_forget(&ask()).await.unwrap();
  assert!(client.session_loads(&ask()).await.unwrap().loads.is_empty());
  assert_eq!(
    refused(
      client
        .session_loads(&SessionAsk {
          session: String::new()
        })
        .await
    )
    .0,
    400
  );
}

#[tokio::test]
async fn an_agents_note_is_kept_overwritten_read_and_cleared() {
  let served = Served::new().await;
  let client = &served.client;
  let ask = |agent: &str| AgentAsk {
    agent: agent.into(),
  };

  assert_eq!(client.session_note(&ask("agent-a")).await.unwrap().note, None);
  client
    .session_note_keep(&NoteKeep {
      agent: "agent-a".into(),
      note: "the first".into(),
    })
    .await
    .unwrap();
  assert_eq!(client.session_note(&ask("agent-a")).await.unwrap().note.as_deref(), Some("the first"));
  client
    .session_note_keep(&NoteKeep {
      agent: "agent-a".into(),
      note: "the second".into(),
    })
    .await
    .unwrap();
  assert_eq!(client.session_note(&ask("agent-a")).await.unwrap().note.as_deref(), Some("the second"));
  assert_eq!(client.session_note(&ask("agent-b")).await.unwrap().note, None);
  client.session_note_clear(&ask("agent-a")).await.unwrap();
  assert_eq!(client.session_note(&ask("agent-a")).await.unwrap().note, None);
  assert_eq!(
    refused(
      client
        .session_note_keep(&NoteKeep {
          agent: "agent-a".into(),
          note: " ".into(),
        })
        .await
    )
    .0,
    400
  );
}

#[tokio::test]
async fn a_round_reads_back_what_it_kept() {
  let served = Served::new().await;
  let client = &served.client;
  let session = SessionAsk {
    session: "s-round".into(),
  };
  let load = Load {
    path: "docs/a.md".into(),
    version: "v1".into(),
  };

  // The loaded-document read a round reaches first, then what it records once an answer succeeds.
  assert!(client.session_loads(&session).await.unwrap().loads.is_empty());
  client
    .session_loads_keep(&LoadsKeep {
      session: "s-round".into(),
      loads: vec![load.clone()],
      at: None,
    })
    .await
    .unwrap();
  assert_eq!(client.session_loads(&session).await.unwrap().loads, [load]);

  // The handoff note a replaced session opens with, and the room's rules it reads.
  client
    .session_note_keep(&NoteKeep {
      agent: "agent-a".into(),
      note: "the handoff".into(),
    })
    .await
    .unwrap();
  assert_eq!(
    client
      .session_note(&AgentAsk {
        agent: "agent-a".into()
      })
      .await
      .unwrap()
      .note
      .as_deref(),
    Some("the handoff")
  );
  client.room_rule_set(&set(-7, 11, "r1", words(42, "no spoilers"))).await.unwrap();
  assert_eq!(
    client.room_rules(&RoomRulesAsk { chat_id: -7 }).await.unwrap().rules,
    [rule("r1", 42, "no spoilers")]
  );

  // A fold lets the loads and the note go.
  client.session_loads_forget(&session).await.unwrap();
  client
    .session_note_clear(&AgentAsk {
      agent: "agent-a".into(),
    })
    .await
    .unwrap();
  assert!(client.session_loads(&session).await.unwrap().loads.is_empty());
  assert_eq!(
    client
      .session_note(&AgentAsk {
        agent: "agent-a".into()
      })
      .await
      .unwrap()
      .note,
    None
  );
}
