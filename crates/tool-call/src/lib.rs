//! The brain's side of the tool host's call protocol (docs/boundary-api.md, 「Its tools」), which every tool of the
//! brain's shares: the one request a placed executable is handed on standard input, the parts of its session
//! record the brain's tools read and the turn they read from it, what a call is answered with, the brain the
//! tools answer through, and the manifest a tool's tests hold to its executable.

mod answer;
mod brain;
mod handed;
mod input;
mod manifest;
mod session;

pub use answer::{Answer, Content};
pub use brain::{ADDRESS_VARIABLE, Brain, ENV_FILE, Said, TOKEN_VARIABLE};
pub use handed::{Asked, Handed, PROTOCOL, answer_standard_io, read_handed};
pub use input::{AtLeastOne, Positive};
pub use manifest::{Entry, differences, input_schema};
pub use session::{AGENT_VARIABLE, CHAT_VARIABLE, Session, Turn};
