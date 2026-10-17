//! The brain's side of the tool host's call protocol: a request read or refused, the turn read from the record, an
//! answer written as the protocol writes one, the input shapes held to their bounds, and the brain found under the
//! data root.

use std::collections::BTreeMap;

use serde_json::json;
use tool_call::{Answer, Asked, AtLeastOne, Brain, ENV_FILE, Positive, Session, Turn, read_handed};

#[test]
fn a_call_is_read_with_its_record_and_another_protocol_or_shape_is_refused() {
  let handed = read_handed(
    json!({
      "protocol": 1,
      "kind": "call",
      "entry": "bmo/memory_list",
      "arguments": { "all": true },
      "session": { "line": "chat", "capabilities": ["room.tools"], "dataRoot": "/data", "archive": "x",
                   "runVariables": { "BMO_TURN_CHAT": "-5" }, "fragmentDesk": "/data/fragment-desks/a.json" },
      "callback": "/run/socket",
    })
    .to_string()
    .as_bytes(),
  )
  .unwrap();

  assert_eq!(
    handed.asked,
    Asked::Call {
      entry: "bmo/memory_list".into(),
      arguments: json!({ "all": true })
    }
  );
  assert_eq!(handed.session.data_root.to_str(), Some("/data"));
  assert_eq!(
    handed.session.fragment_desk.as_deref().and_then(|desk| desk.to_str()),
    Some("/data/fragment-desks/a.json")
  );
  assert_eq!(
    read_handed(
      json!({ "protocol": 1, "kind": "helper", "socket": "/s", "session": {} })
        .to_string()
        .as_bytes()
    )
    .unwrap()
    .asked,
    Asked::Other
  );
  assert!(
    read_handed(
      json!({ "protocol": 2, "kind": "call", "entry": "x", "session": {} })
        .to_string()
        .as_bytes()
    )
    .unwrap_err()
    .contains("protocol 2")
  );
  assert!(read_handed("not json".as_bytes()).unwrap_err().contains("does not read"));
}

#[test]
fn the_turn_is_the_places_run_over_the_records_own_and_a_persons_turn_names_none_from_the_place() {
  let place = tempfile::tempdir().unwrap();
  let record = |run: serde_json::Value| {
    std::fs::write(place.path().join("run.json"), run.to_string()).unwrap();

    Session {
      place: Some(place.path().to_path_buf()),
      run_variables: BTreeMap::from([
        ("BMO_TURN_AGENT".to_owned(), "agent-b".to_owned()),
        ("BMO_TURN_CHAT".to_owned(), " 1 ".to_owned()),
      ]),
      ..Session::default()
    }
  };

  assert_eq!(
    Turn::of(&record(json!({ "env": { "BMO_TURN_AGENT": "agent-a", "BMO_TURN_CHAT": "-5" } }))),
    Turn {
      agent: Some("agent-a".into()),
      chat: Some(-5)
    }
  );
  assert_eq!(
    Turn::of(&record(json!({ "person": true, "env": { "BMO_TURN_AGENT": "agent-a" } }))),
    Turn {
      agent: Some("agent-b".into()),
      chat: Some(1)
    }
  );
  assert_eq!(Turn::of(&Session::default()), Turn::default());
}

#[test]
fn an_answer_is_written_as_the_protocol_writes_one() {
  assert_eq!(
    serde_json::to_value(Answer::refusal("no desk")).unwrap(),
    json!({ "content": [{ "type": "text", "text": "no desk" }], "isError": true })
  );
  assert_eq!(Answer::failure("down").plain(), "ERROR: down");

  let reading = Answer::structured(json!({ "lines": ["one"] }));

  assert_eq!(
    serde_json::to_value(&reading).unwrap(),
    json!({ "content": [{ "type": "text", "text": "{\"lines\":[\"one\"]}" }], "isError": false, "structuredContent": { "lines": ["one"] } })
  );
}

#[test]
fn a_positive_number_and_a_list_of_one_are_held_to_their_bounds() {
  assert!(serde_json::from_value::<Positive>(json!(0)).is_err());
  assert!(serde_json::from_value::<Positive>(json!(1.5)).is_err());
  assert_eq!(serde_json::from_value::<Positive>(json!(758)).unwrap(), Positive(758));
  assert!(serde_json::from_value::<AtLeastOne<String>>(json!([])).is_err());
  assert_eq!(serde_json::from_value::<AtLeastOne<String>>(json!(["a"])).unwrap().0, ["a"]);
}

#[test]
fn the_brain_is_found_in_the_data_roots_env_file_and_its_absence_said() {
  let data = tempfile::tempdir().unwrap();

  assert!(Brain::under(data.path()).unwrap_err().contains("no BRAIN_ADDRESS"));

  std::fs::create_dir_all(data.path().join("env")).unwrap();
  std::fs::write(
    data.path().join(ENV_FILE),
    "# the brain\nBRAIN_ADDRESS=http://127.0.0.1:7/\nBRAIN_TOKEN=\"secret\"\n",
  )
  .unwrap();

  let brain = Brain::under(data.path()).unwrap();

  assert_eq!((brain.base.as_str(), brain.token.as_str()), ("http://127.0.0.1:7", "secret"));
}
