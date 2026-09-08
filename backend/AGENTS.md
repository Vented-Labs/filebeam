# Backend Guidelines

- Target PHP 8.5+ and follow the conventions already used in the affected code.
- Confirm installed package APIs before adding framework- or package-specific code.
- Keep encrypted content and manifest metadata out of API metadata, logs, and server-side inspection.
- Keep schema and storage behavior compatible with SQLite, MySQL/MariaDB, and PostgreSQL unless explicitly documented otherwise.
- Keep enums in `App\Enums`. Put repeated query logic in model methods marked with `#[Scope]`, and consolidate duplicated behavior at its existing owner.
- Use strict types in every first-party PHP file. Preserve type annotations and comments explaining non-obvious constraints, not generated narration.
- Run the narrowest relevant checks. PHP commands live in `backend/`; JavaScript commands run from the repository root.
