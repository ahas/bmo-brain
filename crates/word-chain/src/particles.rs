//! A name joined to its Korean particle, chosen by whether its last syllable closes on a consonant, so a
//! line joining a handed-in name to its particle reads right whatever the name ends on. The particles are
//! the policy's.

use crate::hangul::last_final;
use crate::policy::WORD_CHAIN;

// The final consonant a name's last character is read as closing on, or nothing where its spelling does not
// say: a Hangul syllable's own, and a figure's reading, closed or open as it is read aloud.
fn final_sound(name: &str) -> Option<u32> {
  let last = name
    .trim_end_matches([
      ')', ']', '}', '\u{300D}', '\u{300F}', '"', '\'', '\u{201D}', '\u{2019}', '\u{300B}', '\u{3009}',
    ])
    .chars()
    .last()?;

  match last {
    '\u{AC00}'..='\u{D7A3}' => Some(last_final(&last.to_string())),
    '0' | '1' | '3' | '6' | '7' | '8' => Some(1),
    '0'..='9' => Some(0),
    _ => None,
  }
}

/// A name with its subject particle, both particles given where its sound cannot be read.
pub fn as_subject(name: &str) -> String {
  let particles = &WORD_CHAIN.particles;
  let particle = match final_sound(name) {
    Some(0) => &particles.after_vowel,
    Some(_) => &particles.after_consonant,
    None => &particles.unread,
  };

  format!("{name}{particle}")
}
