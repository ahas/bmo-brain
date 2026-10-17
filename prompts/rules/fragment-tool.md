# The fragment tool

Every piece you write is a row of the brain's fragment table, the pieces' one index: the reference gates,
the writers after you and the console all find a piece by its row. You write a piece's body with
your file tools, and read and write its row with the fragment tool: `fragment_search`,
`fragment_write`, `fragment_remove`, `fragment_merge`, `fragment_split` and `fragment_owed`. The
code draws the index files beside the pieces from the rows: never write or edit an `index.md`.

## Which files are pieces

- The pieces are the files your task tells you to write pieces at: a person-book piece under a
  `chunks/` directory (`c<userId>-<topic>.md`, `p<userId>-<topic>.md`), a room rule chunk beside a
  room's rulebook, a knowledge chunk in the knowledge library.
- Nothing else is a piece or has a row: not a standing sheet, not `knowledge/candidates.md`, not
  an index.

## Finding what already stands

- **Search before you create.** `fragment_search` answers with the rows you may reach (slug,
  title, path, keywords, items, period and status) narrowed by words, an item number or a person.
  Ask it with the words the messages or notes use for the matter, and the item numbers they name.
- A matter a row already holds goes into that piece: open it with your file tools, fold the new
  evidence in, and write its row again. A new piece is for a matter no row holds.
- `fragment_write` refuses a new piece whose items or keywords a standing row shares, and names
  those rows. Read them; put the matter where it belongs, fold the pieces with `fragment_merge`,
  or, only where the matter really is another, write again naming each in `distinct_from`.

## When a row is written

- **Every piece you create or change gets its row**, with `fragment_write`, after its body is on
  disk; a piece changed again later gets its row written again. A piece left alone gets none.
- **A piece you take away** goes with `fragment_remove`, which deletes its file and takes its row
  down. Your file tools cannot delete a file.
- **Merging pieces:** write the merged body into the piece you keep, then `fragment_merge` with it
  as `into` and the others as `from`; the others are removed.
- **Splitting a piece:** write every piece coming out of it, then `fragment_split` with the
  original as `from` and every new piece under `pieces`; the original is removed unless among them.
- **Before you finish**, ask `fragment_owed`. A piece it names has no row, and a run ending with
  one fails whole, everything it wrote put back.
- A refused call wrote nothing. Read why, put it right, and call again.
- The tool refuses a piece with an item whose day bracket leaves its slot names out: every item
  opens `[date D · confirmed D]`, a judgement `[basis D · confirmed D]`, never `[D · confirmed D]`.

## What a row says

- **title**: one or two sentences, 40 to 320 characters, saying what kind of document the piece is,
  the questions or situations a turn needs it for and, where a reader could mistake it, when it is
  not needed. English, with the names and terms the language rule keeps. No slug, no dates, no
  period. Rewrite it whenever the body changes, so it never describes what the piece used to hold.
  For example: `Person-book piece on how the owner wants a job report laid out and worded; needed
  when {{agent}} writes or relays a job report, or the owner asks why a report looks the way it does.`
- **A title is not a list of words.** Nouns strung with commas or middle dots, or a title hung on
  the words nearly every turn carries ({{agent}}, worker, gate, agent, TODO, job, room, the owner's
  name), is matched
  by a gate on every turn, attaching the piece to turns having nothing to do with it. The tool
  refuses a title shorter than 40 characters, a list of nouns, a title mostly of those common
  words, and one naming no situation (when, if, only, {{ko_when}}, {{ko_case}}).
- **The situation is the one question the piece settles, not the room's everyday work.** "Needed
  when writing or reviewing code", "when the owner gives an instruction" or "when a job is discussed"
  fits most turns of the rooms the piece reaches, and a gate, weighing the situation above the rest
  of the line, attaches it to all of them. Name the turn that would go wrong without the piece:
  `needed only when someone asks whether a node name may be written into a file, unit or variable`.
- **A title never carries {{agent}},** and the tool refuses one that does. Every turn calling
  {{agent}} stands on that word, so a title holding it, even once or quoted, is matched by every such
  turn. A piece about how {{agent}} is called or read says "the assistant's name" and leaves the
  spelling to its body.
- **keywords**: one text of three to ten comma-separated words somebody would ask for this piece
  by, particular to it: first the Korean words the room itself uses for the matter, exactly as
  written in the messages or notes, then names, nicknames, product and system names, identifiers.
  Never a whole sentence, and never one of the common words above, which the tool refuses: a
  keyword keeps its piece on every turn holding it, and a common word is held by nearly every turn.
  The one exception is a person's address piece, whose keywords are the names that person is
  called by. For example: `"keywords": "{{ko_address_keywords}}"`.
- **items**: the TODO item numbers the piece is about (`#758` is `758`), or `[]` when none.
- **shared**: on a knowledge chunk only: `true` for a chunk every room may read, `false` for a
  chunk of this chat alone. Leave it out on every other piece.
- Everything else on the row (its room, person, slug, dates and whether its matter is finished)
  the code reads off the path and the body.
