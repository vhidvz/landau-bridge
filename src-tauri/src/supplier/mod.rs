//! Supplier layer: one trait, one implementation today (spec §3, §4).

pub mod barezpakhsh;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}
