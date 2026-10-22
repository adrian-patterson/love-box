//! Optional content source seam; integrations are implemented in later milestones.

pub trait Source {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
}
