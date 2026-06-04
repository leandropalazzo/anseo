ALTER TABLE mentions
    ADD COLUMN sentiment_label TEXT,
    ADD COLUMN sentiment_score SMALLINT,
    ADD COLUMN sentiment_lane TEXT;

ALTER TABLE mentions
    ADD CONSTRAINT mentions_sentiment_label_check
        CHECK (sentiment_label IS NULL OR sentiment_label IN ('positive', 'neutral', 'negative')),
    ADD CONSTRAINT mentions_sentiment_score_check
        CHECK (sentiment_score IS NULL OR sentiment_score BETWEEN 0 AND 100),
    ADD CONSTRAINT mentions_sentiment_lane_check
        CHECK (sentiment_lane IS NULL OR sentiment_lane IN ('deterministic_lexicon', 'non_deterministic'));

UPDATE mentions
SET sentiment_label = 'neutral',
    sentiment_score = 50,
    sentiment_lane = 'deterministic_lexicon'
WHERE sentiment_label IS NULL;
