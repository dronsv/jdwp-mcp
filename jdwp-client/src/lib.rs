// JDWP client library for Java debugging
//
// Implements a subset of the JDWP protocol focused on practical debugging scenarios:
// - Connection management
// - Breakpoint operations
// - Stack inspection
// - Variable evaluation
// - Execution control

pub mod array;
pub mod commands;
pub mod connection;
pub mod eventloop;
pub mod eventrequest;
pub mod events;
pub mod method;
pub mod object;
pub mod protocol;
pub mod reader;
pub mod reftype;
pub mod stackframe;
pub mod string;
pub mod thread;
pub mod types;
pub mod vm;

pub use connection::JdwpConnection;
pub use eventloop::{spawn_event_loop, EventLoopHandle};
pub use eventrequest::{StepDepth, StepSize, SuspendPolicy};
pub use events::EventSet;
pub use protocol::{JdwpError, JdwpResult};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
