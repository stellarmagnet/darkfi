use once_cell::sync::Lazy;
use pasta_curves::pallas;

pub static FUNC_ID: Lazy<pallas::Base> = Lazy::new(|| pallas::Base::from(110));

pub mod validate;
pub mod wallet;
