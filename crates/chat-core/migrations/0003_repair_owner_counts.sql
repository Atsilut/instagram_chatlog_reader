-- SPDX-License-Identifier: AGPL-3.0-or-later

-- Imports before this version marked the owner and updated chat counts only after
-- the whole import finished, so a cancelled import left both unset. Repair them.
UPDATE participants SET is_owner = 1
WHERE sender_key = '@self'
  AND NOT EXISTS (SELECT 1 FROM participants o WHERE o.thread_id = participants.thread_id AND o.is_owner = 1);

UPDATE threads SET
    message_count = (SELECT COUNT(*) FROM messages WHERE thread_id = threads.id),
    last_sort_key = (SELECT MAX(sort_key) FROM messages WHERE thread_id = threads.id);
