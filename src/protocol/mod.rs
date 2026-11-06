//! The application protocol: frame definition, codec and priority router.

pub mod codec;
pub mod frame;
pub mod router;

pub use codec::{decode_envelope, decode_frame, encode_envelope, encode_frame, FrameSink};
pub use frame::{Envelope, Frame, Priority};
pub use router::{MuxRouter, OutboundFrame, RouterConfig};
