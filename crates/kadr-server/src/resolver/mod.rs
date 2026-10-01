pub mod card;
#[allow(clippy::module_inception)]
pub mod resolver;

pub use card::to_card_view_model;
pub use resolver::WidgetResolver;
