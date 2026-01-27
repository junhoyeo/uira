mod agent_memo;
mod bdd;
mod directive;
mod shebang;

pub use agent_memo::AgentMemoFilter;
pub use bdd::BddFilter;
pub use directive::DirectiveFilter;
pub use shebang::ShebangFilter;

use crate::models::CommentInfo;

pub trait CommentFilter {
    fn should_skip(&self, comment: &CommentInfo) -> bool;
}

pub struct FilterChain {
    filters: Vec<Box<dyn CommentFilter>>,
}

impl FilterChain {
    pub fn new() -> Self {
        Self {
            filters: vec![
                Box::new(BddFilter),
                Box::new(DirectiveFilter),
                Box::new(ShebangFilter),
            ],
        }
    }

    pub fn should_skip(&self, comment: &CommentInfo) -> bool {
        self.filters.iter().any(|f| f.should_skip(comment))
    }
}

impl Default for FilterChain {
    fn default() -> Self {
        Self::new()
    }
}
