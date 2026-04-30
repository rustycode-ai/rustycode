use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[non_exhaustive]
pub enum TaskPriority {
    Background = 1,
    Low = 2,
    #[default]
    Normal = 3,
    High = 4,
    Critical = 5,
}

impl TaskPriority {
    pub fn as_i32(&self) -> i32 {
        *self as i32
    }
}
