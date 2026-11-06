//! Room naming rules.
//!
//! Rooms are plain strings so that a school can invent its own without
//! touching the protocol. A handful of conventions are understood by every
//! node:
//!
//! * `dm:<node-a>:<node-b>` is a private conversation between two nodes.
//!   The two ids are sorted, so both sides compute the same room and a
//!   private message is only ever delivered to the two nodes involved.
//! * `stream:<name>` carries a live broadcast.
//! * `reels` carries published clips.
//! * everything else is an ordinary channel that every node may read.

/// The prefix of a private conversation.
pub const DIRECT_PREFIX: &str = "dm:";

/// The prefix of a live broadcast room.
pub const STREAM_PREFIX: &str = "stream:";

/// The room that carries published clips.
pub const REELS_ROOM: &str = "reels";

/// The rooms a node knows about before anybody says anything.
pub const DEFAULT_ROOMS: &[&str] = &["general", "announcements"];

/// What a room is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomKind {
    /// An ordinary channel everybody in the network can read.
    Channel,
    /// A private conversation between two nodes.
    Direct,
    /// A live broadcast.
    Stream,
    /// The reels feed.
    Reels,
}

impl RoomKind {
    /// The label used on the wire.
    pub fn as_str(&self) -> &'static str {
        match self {
            RoomKind::Channel => "channel",
            RoomKind::Direct => "direct",
            RoomKind::Stream => "stream",
            RoomKind::Reels => "reels",
        }
    }
}

/// What kind of room this is.
pub fn room_kind(room: &str) -> RoomKind {
    if room.starts_with(DIRECT_PREFIX) {
        RoomKind::Direct
    } else if room.starts_with(STREAM_PREFIX) {
        RoomKind::Stream
    } else if room == REELS_ROOM {
        RoomKind::Reels
    } else {
        RoomKind::Channel
    }
}

/// Whether this is a private conversation.
pub fn is_direct(room: &str) -> bool {
    room.starts_with(DIRECT_PREFIX)
}

/// The canonical room for a conversation between two nodes.
///
/// The ids are sorted so that both sides - and every browser attached to
/// them - arrive at the same string.
pub fn conversation_room(a: &str, b: &str) -> String {
    let (first, second) = if a <= b { (a, b) } else { (b, a) };
    format!("{DIRECT_PREFIX}{first}:{second}")
}

/// The node a direct room is addressed to, if the room names one.
///
/// A room like `dm:node-a` names the other party. A room like
/// `dm:node-a:node-b` names both and is handled by
/// [`conversation_room`].
pub fn direct_target(room: &str) -> Option<&str> {
    let rest = room.strip_prefix(DIRECT_PREFIX)?;
    if rest.contains(':') {
        None
    } else if rest.is_empty() {
        None
    } else {
        Some(rest)
    }
}

/// A short label for a room, used in the interface.
pub fn room_title(room: &str) -> String {
    match room_kind(room) {
        RoomKind::Direct => {
            let rest = room.strip_prefix(DIRECT_PREFIX).unwrap_or(room);
            match rest.split(':').next_back() {
                Some(name) if !name.is_empty() => name.to_string(),
                _ => "direct message".to_string(),
            }
        }
        RoomKind::Stream => room
            .strip_prefix(STREAM_PREFIX)
            .unwrap_or(room)
            .to_string(),
        RoomKind::Reels => "reels".to_string(),
        RoomKind::Channel => room.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_sides_compute_the_same_private_room() {
        let a = conversation_room("node-a", "node-b");
        let b = conversation_room("node-b", "node-a");
        assert_eq!(a, b);
        assert_eq!(a, "dm:node-a:node-b");
    }

    #[test]
    fn rooms_are_classified() {
        assert_eq!(room_kind("general"), RoomKind::Channel);
        assert_eq!(room_kind("dm:node-a:node-b"), RoomKind::Direct);
        assert_eq!(room_kind("stream:hall"), RoomKind::Stream);
        assert_eq!(room_kind("reels"), RoomKind::Reels);
    }

    #[test]
    fn a_one_sided_room_names_its_target() {
        assert_eq!(direct_target("dm:node-b"), Some("node-b"));
        assert_eq!(direct_target("dm:node-a:node-b"), None);
        assert_eq!(direct_target("general"), None);
    }
}
