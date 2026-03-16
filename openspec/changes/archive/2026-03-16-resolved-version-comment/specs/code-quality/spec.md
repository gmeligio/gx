## Code Conventions

### Requirement: Domain newtypes for semantic string fields [MODIFIED]

#### VersionComment [REMOVED]

~~`VersionComment` SHALL represent a derived version comment (e.g., `"v6"` from specifier `^6`). Defined in `domain::action::identity` (not `domain::lock::resolution`) to avoid a reverse dependency from `domain::action` → `domain::lock`, since `Specifier::Range` in `domain::action::specifier` uses this type. Standard construction via `From<String>` and `From<&str>`.~~

~~##### Scenario: VersionComment is a type-level marker~~
~~- **GIVEN** a `VersionComment` value~~
~~- **THEN** it SHALL wrap a string without validation~~
~~- **BECAUSE** the invariant (comments are derived from specifiers) is enforced at the call site, not in the constructor~~

**Reason**: The `VersionComment` type is deleted. Workflow annotations now use `Resolution.version` (a `Version` newtype) directly. There is no separate "comment" concept in the domain — the resolved version IS the annotation.
