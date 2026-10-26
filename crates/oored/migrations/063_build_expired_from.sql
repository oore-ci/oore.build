-- The outcome a build had before retention moved it to `expired`, so build
-- insights keep counting it as succeeded or failed. NULL for builds that
-- expired for any other reason.
ALTER TABLE builds ADD COLUMN expired_from TEXT;
