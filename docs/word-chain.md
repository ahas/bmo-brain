# The word chain in a room

How a room plays the Korean word-chain game (kkeutmal-itgi) together with an agent: the word book and where it
comes from, the rules a move is judged by, how a game is opened, run and ended, what the agent plays, how a game's
lines reach the room, and why a move costs no model run. The words a room says to a game, and every other piece of
the game's vocabulary in the room's language, are data in `policy/word-chain.toml`; this document names them by
what they do.

## The word book

- **The words are KKuTu's.** KKuTu (`https://github.com/JJoriping/KKuTu`) is an open-source Korean word-game
  server; its repository ships a PostgreSQL dump, `db.sql`, whose `kkutu_ko` table lists about 431,000 Korean
  words with a part-of-speech code and flags for each.
- **Only what a chain game judges by is kept:** the word, and whether it is a loanword. A word is kept where it is
  a noun, is two syllables or more, is spelled in Hangul alone, and carries no flag but the loanword one, so no
  dialect word, old form, North Korean standard word, word written with spaces, or word KKuTu accepts outside the
  dictionary. About 209,000 words remain. The dump's meanings are placeholders and are not read.
- **The book is a SQLite file under the data root,** `word-chain/ko.sqlite`, with each word's first and last
  syllable, and a count per opening syllable of the words that start with it, all and native only, so asking what
  can follow a word is one indexed read.
- **Installing it** is the brain binary's `word-chain install` command: it fetches `db.sql` from the upstream
  commit the reader was written against, then the dictionaries below, and writes the book beside the old one
  before moving it into place. `--from <db.sql>` reads a dump already on the machine, and `--nikl-from <folder>` a
  copy of the mirror's `stdict/` and `krdict/` parts. The book records each source, commit, licence and size in
  itself, and the command reports as the `book-` codes say (`boundary-api.md`, 「Word-chain codes」).
  - The sources, their commits and licences, and the dictionaries' own words the reading goes by (a noun's part of
    speech, the kinds and fields of sense passed over) stand in the policy file.
- **Each word's meaning comes from the National Institute of Korean Language (NIKL).** The installer also reads
  the Standard Korean Language Dictionary (stdict) and the Basic Korean Dictionary for learners (krdict), both as
  the NIKL publishes them for download, from the mirror `https://github.com/spellcheck-ko/korean-dict-nikl` at a
  pinned commit. A word of the book keeps the standard dictionary's first plain noun sense of its
  lowest-numbered homograph, its field (such as Buddhism or chemistry) before it and never a person, place or
  book, and whether the basic dictionary lists it; a word the basic dictionary lists is a basic word.
- **Looking a word up** is the binary's `word-chain word <word>` command: whether the book has it, whether it is a
  loanword, whether it is a basic word, its meaning, and how many words can follow it.
- **The licence.** KKuTu's repository is GPL-3.0; its images and sounds are CC BY, and its English words come
  from WordNet. The book is built on the machine from the upstream dump and is never committed or published, so
  nothing of KKuTu is redistributed; no KKuTu code is used: the rules below are implemented here from the Korean
  orthography rules, and only the word list is read. Anyone the book is ever handed to is handed KKuTu's GPL-3.0
  with it and told where it came from. The NIKL dictionaries are CC BY-SA 2.0 KR; the book records them as its
  meanings' source, and the same holds for them.

## The rules

- **A word follows** where it starts with the last syllable of the word before it, or with that syllable's form
  under the initial-sound rule (dueum beopchik): an initial rieul before the vowels a, ae, o, oe, u and eu reads
  nieun (ra to na, reum to neum); an initial rieul before ya, yeo, ye, yo, yu and i reads silent ieung (ryeok to
  yeok, ri to i); and an initial nieun before yeo, yo, yu and i reads silent ieung (nyeo to yeo). The rule runs one
  way: a word ending in yeok is never answered with a word starting ryeok.
- **Only a word is a move.** A message from the player whose turn it is counts as a move only where it is one word
  of two Hangul syllables or more; anything else they say (a one-syllable reply, a sentence, bare consonants used
  as laughter) is talk.
- **No move is dropped in silence.** Every move is either taken or refused in the room with the reason, so a player
  never waits on a game that heard nothing.
- **A move is refused** where the word does not start where the chain stands (the refusal names the syllables it
  must start with), is not in the book, was already played in this game, is a loanword in a game that turned
  loanwords off, or, in a game played with the manner rule, leaves nothing to follow it. A refused player keeps the
  turn and tries again, within the time left where the game has a limit.
- **One-shot (hanbang).** A word nothing in the book can follow puts the next player out at once rather than
  leaving them a turn nothing answers; the player who played it scores a bonus. Under the manner rule such a word
  is refused instead. A player alone who plays one scores the bonus and goes on from a new word.
- **Time.** A move has no limit: the game waits for the player whose turn it is. A limit is set only where the room
  asks for one when the game is opened, from 10 to 120 seconds a move, and then a player whose time runs down is
  out.
- **A game nobody plays closes.** Where a turn has no limit and ten minutes pass with no word taken in that game,
  it ends and announces the scores so far; nobody is put out for it. Each game in a room keeps its own ten minutes,
  so a busy game beside it keeps nothing open.
- **Rounds.** A game opens on the word the person who asked gave, where they gave one, and the player after them
  moves first; otherwise it opens on a word drawn from the book that leaves at least twenty answers. Every time the
  player whose move it was goes out, play goes on from a fresh drawn word with the next player.
- **Scores.** A word scores ten points a syllable; a one-shot scores thirty; the last player standing in a game of
  two or more scores fifty. The game is over when one player is left, or, for a player alone, when they go out. The
  ranking puts those still standing first, best score first, then those who went out, the last out first.

## A game in a room

- **Opening.** An agent opens a game when a room asks, with its word-chain start tool, which calls the start route
  (`boundary-api.md`, 「The word chain」). Anyone in a room the agent listens to may ask, as for any other game.
  The person who asked joins at once, and the agent joins as a player unless asked to sit out. A word the person
  who asked already said to begin with goes to the tool as the opening word.
- **An opening word that cannot open waits for another.** Where the book lacks it, it is a loanword in a game
  without them, or nothing follows it (a one-shot word), no word is drawn in its place: the room is told why, and
  the person who asked is asked for another. Their next message of one Hangul word of two syllables or more is
  judged the same way, and the game opens on the first that can open it. The word may come after one of the
  policy's start or opening-word words, and a bare start or opening-word word is answered with a reminder that a
  word is wanted; anything else they say is talk. Ten minutes with no usable word fold the game. A start that names
  nobody who asked, with such a word, is refused with the reason.
- **Recruiting.** The room is told the rules of this game and how to play; anyone who types one of the policy's join
  words joins, and play starts after a minute or when a player types a start word. A game nobody but the agent
  joined folds.
- **Several games in a room.** A room may hold any number of games at once, each with its own players, turn, scores
  and clock. A game standing never refuses a start; a start is refused where the person who asked is already in a
  game.
- **One game a person.** A person is in one game of a room at a time, from joining it until it ends, going out
  included; the agent may be in every game at once. Whose word it is is read from who sent it: a person's move,
  start, give-up or stop goes to the one game they are in, and a join word from someone already in a game is not a
  join.
- **Which game a join is for.** With one game recruiting, a join word joins it. With several, the join word is
  followed by the name of a player of the one meant, matched on the start of the name, spaces, case and the
  policy's closing word for a game aside. A bare join word, or a name that fits more than one, is answered with
  the games recruiting and how to name one; a name that fits none leaves the message as talk. A call for players says so while another game is recruiting beside it.
- **Playing.** The order is the order of joining, with the agent last. Only the player whose move it is is heard,
  and only with a move as 「The rules」 says, trailing punctuation aside; everything else in the room is talk and
  goes on as talk. A player types one of the policy's give-up words to give up.
- **Stopping.** A player typing one of the policy's stop phrases stops the game they are in and no other. The
  owner, where in no game, stops the room's one game with it, or with several names the one meant as a join does.
  The word-chain stop tool, which calls the stop route, given a player (whoever asked, or one of the game someone
  in no game named) stops that player's game; given nobody, it stops the room's only game and is refused where
  several stand. A stopped game announces the scores so far.

## The agent as a player

- **The agent's moves come from the book, not a model.** After a short pause it draws up to sixty unplayed words
  that follow and picks by its difficulty:
  - **easy:** the word leaving the most answers, shortest first;
  - **normal:** any of them;
  - **hard:** the word leaving the fewest answers, which is how it plays one-shot words where the rules allow them.
- **The agent goes out** where nothing it may play is left.

## Cheap and fast

- **A game costs one model run: the turn that opened it.** The receiver offers every message of a room where a game
  stands to the line route before it routes the message, after the message is archived and before an owner's
  command is read, on every road alike. A message already at or before the room's read position is never a move.
  Each is judged against the book in well under a millisecond; the agent's own move takes tens of milliseconds. A
  message the route answers as taken wakes no session, even in the owner's one-to-one or while the room's window
  is open; a message it does not take goes on exactly as it would have.
- **The games live in the brain's process.** The tools reach them through the start and stop routes and the
  receiver through the line route. A restart of the brain ends every standing game without an announcement, and a
  room forgotten lets its games go.
- **What the game says goes out as codes on the room's own road,** with no sanitiser and no formatting gate: every
  line is a code of the game's own with the player names and the Hangul words the book judged as its parameters.
  Everything one step says (a word taken and the next turn, a one-shot and the new round) goes out as one message
  through the receiver's coded room line, which archives it as the agent's own, so a session woken later reads
  what happened.

## How it looks in the room

- **The lines are the web unit's language pack's** word-chain part. The brain sends each line's code and its
  parameters (`boundary-api.md`, 「Word-chain codes」); the pack renders the sentence and chooses a line's
  particles by the sound a word ends on, so a word or a name is sent bare.
- **Plain text on every road.** The lines carry no markup, emoji or links, only corner brackets around words and
  syllables, square brackets around a game's name and line breaks between the parts of a step, so a room on a road
  that shows no formatting and a room on one that does read the same message.
- **A game is named by its players while another stands beside it.** Where a room holds more than one game, every
  message of a game opens on a line naming it by the people in it, the agent aside, in the order they joined: the
  first three and a count of the rest where there are more; a game with nobody but the agent is the agent's. A
  room's only game is not named.
- **A word that is not basic comes with its meaning.** Where the word a round opens on or a word a player, the
  agent included, played is not a basic word and the book has its meaning, the line after it gives that meaning in
  one line of at most about a hundred characters.

## Where it stands in the code

- **The game is its own crate, `word-chain`,** holding the Hangul arithmetic, the word book, and the game
  (players, turns, judging, rounds, scores and the robot), which keeps time with a clock it is handed and says what
  happened as events. It names no room, road or agent. The KKuTu and NIKL readers that build the book are the
  install command's.
- **The Hangul arithmetic is done on code points:** a syllable is taken apart and put together by its place in
  the Unicode syllable block, and the initial-sound rule is a table of consonant and vowel indices, so no syllable
  of the language is written in the code. The crate also reads which subject particle a name takes by the sound it
  ends on, for a caller that joins a name to its particle itself; the particles are the policy's.
- **The service holds the rest:** which room each game stands in and which game each person is in, the wait for a
  usable opening word, which words of a room are joins, starts, give-ups and stops and which game each is for, the
  name a game goes by, the code each event leaves as, and the routes.
- **What a game is played by is the policy's:** the words a room says to a game, how long a game recruits, waits
  for a move and lets the agent think, how many answers a round's opening word must leave and the syllables it is
  drawn from, how many words the agent weighs, the range a turn's seconds are held to, how many players a game's
  name lists and the closing word a name given for a game may end with, the points, and the subject particles a
  name takes after a vowel, after a consonant and where its sound cannot be read.
