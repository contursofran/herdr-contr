//! herdr-contr's orchestration tool (#160), ported from the contr repository's Go code: the profiles and the
//! flow that a coordinator follows (`config`), the workers in hc terminals (`workers`), and the questions and
//! to-dos that wait for the owner (`owner`).
//! The files and their places are the same as contr's, so both tools read the same state during the port.
pub mod cli;
pub mod config;
pub mod jsonfile;
pub mod owner;
pub mod workers;
