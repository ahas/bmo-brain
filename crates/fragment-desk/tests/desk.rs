//! The desk file as it is read and written.

use fragment_desk::desk::FragmentFilter;

#[test]
fn a_room_left_out_and_a_room_of_none_stay_apart() {
  let every: FragmentFilter = serde_json::from_str(r#"{"kind":"person-book"}"#).unwrap();
  let none: FragmentFilter = serde_json::from_str(r#"{"kind":"person-book","chatId":null}"#).unwrap();

  assert_eq!(every.chat_id, None);
  assert_eq!(none.chat_id, Some(None));
  assert_eq!(serde_json::to_string(&none).unwrap(), r#"{"kind":"person-book","chatId":null}"#);
}
