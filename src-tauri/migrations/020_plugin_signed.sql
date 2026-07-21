-- Whether a plugin was installed from a signature-verified .syzify-ext package.
-- Set only by the package installer; unsigned dev sideloads stay 0.
ALTER TABLE plugin ADD COLUMN signed INTEGER NOT NULL DEFAULT 0;
