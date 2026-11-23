//! The models, one module each.
//!
//! | Module | Index | Memory |
//! |---|---|---|
//! | [`risico`] | RISICO: fine fuel moisture, rate of spread, fireline intensity | yes |
//! | [`fwi`] | Canadian Fire Weather Index System | yes |
//! | [`kbdi`] | Keetch–Byram Drought Index | yes |
//! | [`mark5`] | McArthur Forest Fire Danger Index, Mark 5 | yes |
//! | [`angstrom`] | Angström index | no |
//! | [`fosberg`] | Fosberg Fire Weather Index | no |
//! | [`nesterov`] | Nesterov ignition index | yes |
//! | [`sharples`] | Sharples fuel moisture and fire danger indices | no |
//! | [`orieux`] | Orieux water reserve and fire danger class | yes |
//! | [`hdw`] | Hot–Dry–Windy Index | no |
//!
//! Each module has a `models` submodule with its properties, warm state and
//! state, a `config` submodule where the model has versions, and the
//! formulas in `functions`.

pub mod functions;
pub mod fwi;
pub mod risico;
pub mod mark5;
pub mod kbdi;
pub mod angstrom;
pub mod fosberg;
pub mod nesterov;
pub mod sharples;
pub mod orieux;
// pub mod portuguese;  -> UNDER CONSTRUCTION
pub mod hdw;