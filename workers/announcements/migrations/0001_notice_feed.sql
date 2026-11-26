CREATE TABLE notice_feed (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  revision INTEGER NOT NULL CHECK (revision BETWEEN 0 AND 9007199254740991),
  announcements TEXT NOT NULL
) STRICT;

INSERT INTO notice_feed (singleton, revision, announcements) VALUES (1, 0, '[]');
