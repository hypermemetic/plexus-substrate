mod executor;
mod activation;
mod methods;
mod types;
pub mod typed_methods;

pub use executor::BashExecutor;
pub use activation::Bash;
pub use methods::BashMethod;
pub use types::{BashError, BashOutput};
