//! herdr-contr's orchestration tool (#160), ported from the contr repository's Go code: the profiles and the
//! flow that a coordinator follows (`config`), the workers in hc terminals (`workers`) their start (`spawn`) and the menu that a blocked one shows (`prompt`), and the questions and
//! to-dos that wait for the owner (`owner`).
//! The files and their places are the same as contr's, so both tools read the same state during the port.
pub mod cli;
pub mod config;
pub mod flowseen;
pub mod hook;
pub mod jsonfile;
pub mod mcp;
pub mod owner;
pub mod prompt;
pub mod spawn;
pub mod tools;
pub mod turn;
pub mod workers;
