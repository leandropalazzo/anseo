-- Story 12.2 hardening — enforce one prompt per (project, name).
--
-- The Phase 1 schema indexed `(project_id, name)` but didn't enforce
-- uniqueness. Without that, two concurrent `ogeo prompt add vector-db`
-- calls (or a CLI race during YAML import) silently land two rows with
-- the same name, after which `PromptRepo::find_by_name`'s
-- `fetch_optional` returns Err ("expected at most one row") and the
-- API write handler 500s every subsequent POST /v1/prompt-runs for
-- that prompt.
--
-- The slug semantics already imply uniqueness; codifying it in the
-- schema makes the invariant load-bearing.

CREATE UNIQUE INDEX IF NOT EXISTS prompts_project_name_unique
    ON prompts (project_id, name);
