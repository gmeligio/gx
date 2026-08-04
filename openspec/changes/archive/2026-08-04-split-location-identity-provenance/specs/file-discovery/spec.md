## ADDED Requirements

### Requirement: A file is rewritten with its own pins

gx SHALL pair each discovered file with the managed references found in that
same file, matched by exact path. gx SHALL NOT pair a file with references
found in a different file, and the pairing SHALL NOT depend on map or
filesystem enumeration order.

**User value:** a user with a composite action nested inside another composite
action's directory runs `gx tidy` and each file is pinned from its own
references. Before this, the file-to-references lookup matched by path suffix
and took the first hit from an unordered map, so when one file's path ended
with another's, a file could be rewritten using the wrong file's pins — writing
a version the user never referenced there, with nothing reporting it. Repeating
the run could produce a different result.

This is the pairing counterpart to "Every discovered file is a candidate for
rewriting": that requirement guarantees every file gets written, this one
guarantees it gets written with the right content.

#### Scenario: Nested composite action directories are paired independently
- **GIVEN** `.github/actions/build/action.yml` uses `actions/checkout@v4`
- **AND** `.github/actions/x/.github/actions/build/action.yml` uses
  `actions/setup-node@v4`
- **WHEN** the user runs `gx tidy`
- **THEN** `.github/actions/build/action.yml` is rewritten with the
  `actions/checkout` pin only
- **AND** `.github/actions/x/.github/actions/build/action.yml` is rewritten with
  the `actions/setup-node` pin only

#### Scenario: Pairing is stable across runs
- **GIVEN** a repository containing two managed files whose paths share a suffix
- **WHEN** the user runs `gx tidy` twice on unchanged inputs
- **THEN** both runs rewrite each file with the same pins

#### Scenario: A file with no managed references is not rewritten
- **GIVEN** a discovered composite action file with no `uses:` references
- **AND** another discovered file whose path ends with that file's path
- **WHEN** the user runs `gx tidy`
- **THEN** the file with no references is left unchanged
- **AND** it is not counted in the summary of files updated
