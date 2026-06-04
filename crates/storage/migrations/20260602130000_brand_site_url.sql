-- Brand site URL — the owned website the brand is monitored for. Powers the
-- /audit prefill and crawler "connect a source" framing. Optional; nullable so
-- existing projects need no backfill.
ALTER TABLE projects
    ADD COLUMN site_url TEXT DEFAULT NULL;
