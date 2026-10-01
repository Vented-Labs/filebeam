# AGENTS.md

Backend work follows `backend/AGENTS.md`.

## Repository Hygiene

- Treat every tracked file as public-facing project material. Keep prose concise,
  professional, current, and useful to users or contributors.
- All agent working material belongs exclusively in repository-root `.filebeam/`:
  plans, TODO lists, notes, handoffs, prompts, investigation findings, evidence,
  status ledgers, implementation reports, command output, logs, review captures,
  and temporary validation or diagnostic scripts.
- Never put working material in `docs/`, READMEs, source comments, `PLAN.md`,
  `notes/`, `tmp/`, or tool-specific directories. Requests for plans, reports, or
  validation do not authorize adding their working files to Git.
- Documentation explains current behavior and reproducible procedures. Omit task
  history, completion claims, agent assignments, and duplicated explanations.
  Comments explain non-obvious constraints, not the work performed.
- Keep only reusable build, CI, release, and development tools in `scripts/`.
  Agent tools and their outputs must stay under `.filebeam/`; scripts must not
  write logs, captures, or reports into documentation or source directories.
- Builds and normal checks must work without a developer's `.filebeam/` contents.
  Never link public documentation to scratch files or force-add them to Git.
- Keep project conventions in concise `AGENTS.md` files. Nested instructions
  must follow this policy; tool-specific prompts and scaffolding stay local.
- Search tools skip ignored paths. Read `.filebeam/` directly for earlier work;
  do not paste its contents into commits, PR descriptions, or code comments.
- Before finishing, inspect the diff and new files for working material,
  machine-specific paths, and unnecessary prose. Move working material into
  `.filebeam/` before it can be committed.
