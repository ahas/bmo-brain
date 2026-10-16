//! The subject particle a name takes, with the names and the subjects they make read from
//! `fixtures/word-chain/particles.toml`.

use serde::Deserialize;
use word_chain::particles::as_subject;

#[derive(Deserialize)]
struct Fixture {
  subjects: Vec<Subject>,
}

#[derive(Deserialize)]
struct Subject {
  name: String,
  subject: String,
}

#[test]
fn a_name_takes_its_particle_by_the_sound_it_ends_on() {
  let fixture: Fixture = toml::from_str(include_str!("fixtures/word-chain/particles.toml")).unwrap();

  for case in fixture.subjects {
    assert_eq!(as_subject(&case.name), case.subject);
  }
}
