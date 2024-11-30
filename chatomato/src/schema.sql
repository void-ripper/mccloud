
CREATE TABLE user (
    id INTEGER PRIMARY KEY,
    pubkey BLOB NOT NULL,
    name VARCHAR
);

CREATE TABLE user_message (
    id INTEGER PRIMARY KEY,
    from_user INTEGER REFERENCES user(id),
    to_user INTEGER REFERENCES user(id),
    message BLOB NOT NULL
);

CREATE TABLE room (
    id INTEGER PRIMARY KEY,
    creator_id INTEGER NOT NULL REFERENCES user(id),
    name VARCHAR
);

CREATE TABLE room_message (
    id INTEGER PRIMARY KEY,
    room_id INTEGER REFERENCES room(id),
    user_id INTEGER REFERENCES user(id),
    message TEXT NOT NULL
);

