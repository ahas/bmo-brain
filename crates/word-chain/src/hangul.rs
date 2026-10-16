//! Hangul syllable arithmetic the word chain needs, on code points: taking a syllable apart, putting one
//! together, and the initial-sound rule (dueum beopchik) by which a word ending in ryeok may be answered by
//! one starting with yeok.

const SYLLABLE_FIRST: u32 = 0xac00;
const SYLLABLE_LAST: u32 = 0xd7a3;
const VOWELS: u32 = 21;
const FINALS: u32 = 28;

// Initial consonant indices, in the order the Unicode syllable block counts them.
const NIEUN: u32 = 2;
const RIEUL: u32 = 5;
const IEUNG: u32 = 11;

// Medial vowel indices the initial-sound rule turns on.
const A: u32 = 0;
const AE: u32 = 1;
const YA: u32 = 2;
const YEO: u32 = 6;
const YE: u32 = 7;
const O: u32 = 8;
const OE: u32 = 11;
const YO: u32 = 12;
const U: u32 = 13;
const YU: u32 = 17;
const EU: u32 = 18;
const I: u32 = 20;

// Rieul before these vowels is read nieun at the head of a word: ra to na, rae to nae, ro to no, roe to
// noe, ru to nu, reu to neu.
const RIEUL_TO_NIEUN: [u32; 6] = [A, AE, O, OE, U, EU];
// Rieul before these vowels drops to a silent ieung: rya to ya, ryeo to yeo, rye to ye, ryo to yo, ryu to
// yu, ri to i.
const RIEUL_TO_IEUNG: [u32; 6] = [YA, YEO, YE, YO, YU, I];
// Nieun before these vowels drops to a silent ieung: nyeo to yeo, nyo to yo, nyu to yu, ni to i.
const NIEUN_TO_IEUNG: [u32; 4] = [YEO, YO, YU, I];

#[derive(Clone, Copy)]
struct Parts {
  initial: u32,
  vowel: u32,
  last: u32,
}

fn split(syllable: char) -> Option<Parts> {
  let code = u32::from(syllable);

  if !(SYLLABLE_FIRST..=SYLLABLE_LAST).contains(&code) {
    return None;
  }

  let offset = code - SYLLABLE_FIRST;

  Some(Parts {
    initial: offset / (VOWELS * FINALS),
    vowel: (offset / FINALS) % VOWELS,
    last: offset % FINALS,
  })
}

fn join(parts: Parts) -> char {
  char::from_u32(SYLLABLE_FIRST + (parts.initial * VOWELS + parts.vowel) * FINALS + parts.last)
    .unwrap_or('\u{fffd}')
}

/// Whether every character of the text is a complete Hangul syllable.
pub fn is_hangul_word(text: &str) -> bool {
  !text.is_empty() && text.chars().all(|ch| split(ch).is_some())
}

/// The syllable a word may start with in place of the given one under the initial-sound rule, or
/// nothing where the rule does not touch it. The rule runs one way only: ryeok may be answered with yeok,
/// never yeok with ryeok.
pub fn initial_sound_variant(syllable: char) -> Option<char> {
  let parts = split(syllable)?;
  let initial = match parts.initial {
    RIEUL if RIEUL_TO_NIEUN.contains(&parts.vowel) => NIEUN,
    RIEUL if RIEUL_TO_IEUNG.contains(&parts.vowel) => IEUNG,
    NIEUN if NIEUN_TO_IEUNG.contains(&parts.vowel) => IEUNG,
    _ => return None,
  };

  Some(join(Parts { initial, ..parts }))
}

/// Every syllable a word may start with to follow a word ending in the given one: itself first.
pub fn openings_for(syllable: char) -> Vec<String> {
  std::iter::once(syllable)
    .chain(initial_sound_variant(syllable))
    .map(String::from)
    .collect()
}

/// The final consonant of a word's last syllable: 0 for none, 8 for rieul, and 0 for a word that does not
/// end in a Hangul syllable.
pub fn last_final(word: &str) -> u32 {
  word.chars().last().and_then(split).map_or(0, |parts| parts.last)
}
