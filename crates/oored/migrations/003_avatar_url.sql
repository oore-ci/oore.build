-- Add avatar URL column to users table for OIDC profile pictures.
ALTER TABLE users ADD COLUMN avatar_url TEXT;
