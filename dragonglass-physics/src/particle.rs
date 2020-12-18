use crate::{Real, Vector3};

#[derive(Default)]
pub struct Particle {
    pub position: Vector3,
    pub velocity: Vector3,
    pub acceleration: Vector3,
    /// The amount of damping applied to linear motion.
    /// Damping is required to remove energy added through
    /// numerical instability in the integrator.
    ///
    /// The damping parameter controls how much velocity is left after the
    /// update. If the damping is zero then the velocity will be reduced to nothing, meaning
    /// that the object couldn’t sustain any motion without a force and would look odd to
    /// the player. A value of 1 means that the object keeps all its velocity (equivalent to no
    /// damping). If you don’t want the object to look like it is experiencing drag, but still
    /// want to use damping to avoid numerical problems, then values slightly less than 1 are
    /// optimal. A value of 0.999 might be perfect, for example.
    pub damping: Real,

    /// Holds the inverse of the mass of the particle.
    ///
    /// It is more useful to hold the inverse mass because
    /// integration is simpler, and because in real-time
    /// simulation it is more useful to have objects with
    /// infinite mass (immovable) than zero mass
    /// (completely unstable in numerical simulation).
    pub inverse_mass: Real,
}

impl Particle {
    /// Integrates the particle forward in time by the given amount.
    /// This function uses a Newton-Euler integration method, which is a
    /// linear approximation to the correct integral. For this reason it
    /// may be inaccurate in some cases.
    pub fn integrate(&mut self, duration: Real) {
        if self.inverse_mass <= 0.0 {
            return;
        }

        // FIXME: Return a real error here instead of panicking
        assert!(duration > 0.0);

        // Update linear position
        self.position += self.velocity * duration;

        // Work out the acceleration from the force
        let acceleration = self.acceleration; // TODO: After force generation this will be added to

        let drag = duration.powf(self.damping);

        // Update linear velocity from the acceleration
        self.velocity += acceleration * duration * drag;

        // Clear any accumulated forces
    }
}
