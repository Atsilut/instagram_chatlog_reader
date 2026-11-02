-- SPDX-License-Identifier: AGPL-3.0-or-later

-- A hidden chat stays in the library and off the main list. Searches that are
-- not limited to that chat skip it. Deleting a chat removes the row entirely.
ALTER TABLE threads ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;
