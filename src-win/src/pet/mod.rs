//! The portable pet logic - mood/stat decay, the behaviour brain, the courier
//! walk state machine, dialogue pools, and the pixel-grid sprites. These modules
//! are pure (no Win32) and carry the ported unit tests from
//! `Tests/ClaudePetTests/`.

pub mod brain;
pub mod courier;
pub mod dialogue;
pub mod pet_state;
pub mod sprites;
