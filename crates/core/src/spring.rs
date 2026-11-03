//! ErisDock's springs, and the sets of them a menu moves with.

use core::f64::consts::{PI, TAU};

use crate::math;

/// A damped spring, set up as SwiftUI's are: `response` is the period of the undamped
/// oscillation in seconds and `damping` the damping fraction, 1 being critical. Retargeting
/// keeps the velocity, so an interrupted motion turns around instead of starting over.
#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub value: f64,
    pub target: f64,
    pub velocity: f64,
    pub response: f64,
    pub damping: f64,
    /// How close to the target, in the value's own units, counts as there.
    pub epsilon: f64,
}

impl Spring {
    pub const fn new(value: f64, spec: SpringSpec) -> Self {
        Self { value, target: value, velocity: 0.0, response: spec.response, damping: spec.damping, epsilon: 0.0005 }
    }

    pub const fn with_epsilon(mut self, epsilon: f64) -> Self {
        self.epsilon = epsilon;
        self
    }

    pub fn is_resting(&self) -> bool {
        self.value == self.target && self.velocity == 0.0
    }

    pub fn animate(&mut self, target: f64) {
        self.target = target;
    }

    /// Jumps to `value` and stays there.
    pub fn set(&mut self, value: f64) {
        self.value = value;
        self.target = value;
        self.velocity = 0.0;
    }

    pub fn step(&mut self, dt: f64) {
        if self.is_resting() {
            return;
        }
        let stiffness = (TAU / self.response) * (TAU / self.response);
        let friction = 4.0 * PI * self.damping / self.response;
        let mut left = dt;
        while left > 0.0 {
            let h = left.min(1.0 / 600.0);
            self.velocity += (-stiffness * (self.value - self.target) - friction * self.velocity) * h;
            self.value += self.velocity * h;
            left -= h;
        }
        if (self.value - self.target).abs() < self.epsilon && self.velocity.abs() < self.epsilon * 20.0 {
            self.set(self.target);
        }
    }
}

/// A spring's response (period in seconds) and damping fraction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringSpec {
    pub response: f64,
    pub damping: f64,
}

impl SpringSpec {
    pub const fn new(response: f64, damping: f64) -> Self {
        Self { response, damping }
    }

    /// From Material's stiffness, for a unit mass.
    pub fn stiffness(stiffness: f64, damping: f64) -> Self {
        Self { response: TAU / math::sqrt(stiffness), damping }
    }
}

/// A set of animations, picked as a whole, named after ErisDock's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    #[default]
    Squishy,
    Expressive,
    Standard,
}

/// How each part of a menu moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Animation {
    /// The body unrolling from the corner it opens from: down (or up) and across.
    pub unroll: SpringSpec,
    pub widen: SpringSpec,
    /// The body's size, as a fraction of its full size, when it starts to open.
    pub collapsed_width: f64,
    pub collapsed_height: f64,
    /// The rows fading in.
    pub reveal: SpringSpec,
    /// The highlight sliding from row to row, and stretching while it does.
    pub slide: SpringSpec,
    /// How far, in points per point-per-second, the moving highlight stretches.
    pub stretch: f64,
}

impl Style {
    pub fn animation(self) -> Animation {
        match self {
            Style::Squishy => Animation {
                unroll: SpringSpec::new(0.3, 0.62),
                widen: SpringSpec::new(0.34, 0.66),
                collapsed_width: 0.55,
                collapsed_height: 0.08,
                reveal: SpringSpec::new(0.22, 1.0),
                slide: SpringSpec::new(0.24, 0.66),
                stretch: 0.012,
            },
            Style::Expressive => Animation {
                unroll: SpringSpec::stiffness(800.0, 0.7),
                widen: SpringSpec::stiffness(800.0, 0.75),
                collapsed_width: 0.8,
                collapsed_height: 0.2,
                reveal: SpringSpec::stiffness(1600.0, 1.0),
                slide: SpringSpec::stiffness(800.0, 0.6),
                stretch: 0.006,
            },
            Style::Standard => Animation {
                unroll: SpringSpec::stiffness(1400.0, 0.95),
                widen: SpringSpec::stiffness(1400.0, 1.0),
                collapsed_width: 1.0,
                collapsed_height: 0.6,
                reveal: SpringSpec::stiffness(1600.0, 1.0),
                slide: SpringSpec::stiffness(1400.0, 0.9),
                stretch: 0.0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(spec: SpringSpec) -> f64 {
        let mut spring = Spring::new(0.0, spec);
        spring.animate(1.0);
        (0..240).fold(0.0, |peak: f64, _| {
            spring.step(1.0 / 120.0);
            peak.max(spring.value)
        })
    }

    #[test]
    fn comes_to_rest_on_its_target() {
        let mut spring = Spring::new(0.0, SpringSpec::new(0.4, 0.6));
        spring.animate(1.0);
        for _ in 0..360 {
            spring.step(1.0 / 120.0);
        }
        assert!(spring.is_resting());
        assert_eq!(spring.value, 1.0);
    }

    #[test]
    fn underdamped_overshoots_by_the_textbook_amount() {
        // exp(-ζπ/√(1-ζ²)) ≈ 9.5% for ζ = 0.6.
        let p = peak(SpringSpec::new(0.4, 0.6));
        assert!((p - 1.095).abs() < 0.01, "peak {p}");
    }

    #[test]
    fn the_styles_unroll_from_most_to_least_bounce() {
        let [squishy, expressive, standard] =
            [Style::Squishy, Style::Expressive, Style::Standard].map(|s| peak(s.animation().unroll));
        assert!(squishy > expressive && expressive > standard, "{squishy} {expressive} {standard}");
        assert!(standard < 1.005);
    }

    #[test]
    fn material_stiffness_becomes_a_response() {
        let spec = SpringSpec::stiffness(380.0, 0.8);
        assert!((spec.response - 0.3223).abs() < 1e-3, "{}", spec.response);
    }
}
