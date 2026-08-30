# Experimental sentence deletion

Status: experimental interaction contract

Last reviewed: 2026-08-30

`Delete sentence` removes the grammatical sentence containing the cursor. A
selection removes every sentence it touches as one edit after deletion ranges
are merged. The default shortcut is `Primary+Shift+U`. The Commands overlay and
the configurable `keybindings.delete_sentence` chord suffix provide fallbacks.
`Primary+U` remains `Delete logical line`.

This command is intentionally experimental. Plain text does not contain enough
information to identify grammatical sentences with certainty.

## Boundary profile

Proqi implements a declared UAX29-C3-2 profile of Unicode Standard Annex 29:

1. Split the exact text at blank-line structural separators. A separator begins
   with an LF, CRLF, or lone CR and contains a second such newline after only
   whitespace. Repeated blank lines form one separator. The separator bytes are
   never passed into a neighboring paragraph's sentence segmenter.
2. Within each nonblank paragraph, create a temporary segmentation shadow.
   Replace every CR and LF byte with one ASCII space. This preserves every UTF-8
   byte offset, including the two bytes of CRLF, while preventing a single prose
   newline from activating the default UAX 29 hard break rules.
3. Run `unicode-segmentation` sentence-boundary iteration on that shadow. Map
   its byte ranges directly back to the unchanged canonical text.
4. Treat each non-whitespace UAX 29 segment as one sentence unit. Punctuation-only
   segments are retained because dropping them would require another linguistic
   heuristic.

This is a programmatic override permitted by UAX29-C3-2, not a fork of the
Unicode sentence rules. UAX 29 still determines breaks around terminators,
closing punctuation, abbreviations, decimal punctuation, scripts, combining
sequences, and emoji. Only newline structure is profiled.

## Exact ownership and deletion

Sentence ownership is deterministic and half open:

- Paragraph-leading whitespace belongs to the first sentence.
- Whitespace between two sentence cores belongs to the preceding sentence.
- Paragraph-trailing whitespace belongs to the final sentence.
- A terminator and any closing punctuation remain in the UAX 29 segment that
  precedes its boundary.
- A cursor exactly at the next sentence's first non-whitespace byte chooses the
  next sentence. End of text chooses the final sentence.
- A cursor inside a blank-line separator chooses the preceding sentence. A
  leading separator with no preceding sentence chooses the following sentence.
- A nonempty selection touches a sentence only when its half-open selected range
  intersects that sentence's owned range. A selection ending exactly at the
  next sentence start does not include that sentence.

Deletion removes the selected sentence core plus its following owned separator.
For the final sentence in a paragraph, it removes the preceding separator
instead. Deleting the only sentence removes all nonstructural whitespace in
that paragraph. Blank-line separators remain exact. All computed ranges are
sorted and merged before one `TextChangeSet` is applied. Every byte outside the
merged ranges remains unchanged.

The editor's existing change transaction rebases unaffected annotations and
folds, dissolves annotations intersected by deletion, collapses the selection to
the first deleted byte, and supplies the same persistent revision path used by
ordinary editor undo and redo. Visual wrapping and resize never participate in
sentence resolution.

## Known ambiguity

The command makes no claim of linguistic certainty:

- Abbreviations can split unexpectedly. Default UAX 29 may treat `Dr.` or `Mr.`
  as a complete segment before an uppercase word.
- URLs, versions, and decimals often remain intact, but surrounding punctuation
  can change the result.
- Source code punctuation can form sentence boundaries that do not match code
  structure.
- Quotations and closing punctuation follow default UAX 29 rules. Semantic
  attribution of a quote is unavailable in plain text.
- Ellipses and punctuation-only text can form their own segments.
- Unterminated text is one sentence within its paragraph.
- Scripts without routine sentence terminators remain one sentence until UAX 29
  finds another boundary.
- Locale-sensitive abbreviation suppression is unavailable because Proqi does
  not know the content language and has no locale preference UI.

Users should use undo when a boundary is surprising. `Delete logical line`
remains the exact nonlinguistic alternative.

## Research and rejected alternatives

- [Unicode Standard Annex 29](https://unicode.org/reports/tr29/) defines default
  sentence boundaries, their acknowledged ambiguity, conformance profiles, and
  the hard CR, LF, and separator behavior that requires Proqi's declared
  newline override.
- [`unicode-segmentation`](https://github.com/unicode-rs/unicode-segmentation)
  is already a direct Proqi dependency. Version 1.13.3 implements Unicode 17.0
  sentence boundaries, is tested against Unicode data, is actively maintained,
  and is dual licensed under Apache-2.0 and MIT. Reusing it keeps one Unicode
  boundary implementation for Proqi.
- [ICU4X sentence segmentation](https://docs.rs/icu_segmenter/latest/icu_segmenter/struct.SentenceSegmenter.html)
  supports content-locale tailoring and maintained Unicode data. It was rejected
  for this action because Proqi has no trustworthy content locale, locale UI is
  out of scope, and its default hard newline behavior still needs the same
  paragraph profile. Adding its data and dependency surface would not solve the
  settled requirement.
- A private punctuation regular expression or copied Unicode rule table was
  rejected. It would drift from Unicode, mishandle non-Latin terminators, and
  create an unmaintained fork.
- Semantic NLP and language detection were rejected because they require a
  larger, probabilistic, often network-backed contract and cannot preserve the
  local, immediate editor boundary.
- [GNU Emacs sentence commands](https://www.gnu.org/software/emacs/manual/html_node/emacs/Sentences.html)
  use directional movement and killing. They stop at a sentence edge rather
  than deleting the whole containing sentence, so their cursor semantics do not
  match this user story.
- [Neovim sentence text objects](https://github.com/neovim/neovim/blob/master/runtime/doc/motion.txt)
  establish useful containing-unit and surrounding-whitespace precedent, but
  their fixed period, exclamation mark, and question mark definition is not a
  Unicode-aware boundary implementation.
- [Visual Studio Code's documented editor actions](https://code.visualstudio.com/docs/reference/default-keybindings)
  provide logical-line deletion and configurable commands but no established
  whole-sentence deletion action to adopt.
