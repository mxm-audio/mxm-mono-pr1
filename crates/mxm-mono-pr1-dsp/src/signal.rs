//! Distinct control-rate signal classes.
//!
//! Continuous CV, held gate level and one-sample trigger pulse are not interchangeable routing
//! values. The fixed Pro-One graph accepts only `Cv`; articulation consumes `Gate` and `Pulse`.

use crate::finite_or;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Cv(f32);

impl Cv {
    pub const fn zero() -> Self {
        Self(0.0)
    }

    pub fn new(value: f32) -> Self {
        Self(finite_or(value, 0.0))
    }

    pub fn value(self) -> f32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Gate(pub bool);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pulse(pub bool);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cv_rejects_nonfinite_values_at_the_plain_value_boundary() {
        assert_eq!(Cv::new(f32::NAN).value(), 0.0);
        assert_eq!(Cv::new(f32::INFINITY).value(), 0.0);
        assert_eq!(Cv::new(-0.75).value(), -0.75);
    }
}
